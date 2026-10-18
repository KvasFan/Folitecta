//! Push buttons: the pictures they show when up, pressed and pointed at,
//! and how they react to a click (their highlight mode).

use lopdf::{Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::forms::FieldKind;
use crate::images::image_object;

/// A state of a push button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Up,
    Down,
    Rollover,
}

impl ButtonState {
    pub const ALL: [ButtonState; 3] = [ButtonState::Up, ButtonState::Down, ButtonState::Rollover];

    /// The key of the picture in the widget's `/MK`.
    fn key(self) -> &'static str {
        match self {
            ButtonState::Up => "I",
            ButtonState::Down => "IX",
            ButtonState::Rollover => "RI",
        }
    }
}

/// How a push button reacts to a click (`/H`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonBehaviour {
    /// Nothing changes.
    None,
    /// The colours are inverted.
    Invert,
    /// The border is inverted.
    Outline,
    /// The pressed look is shown.
    Push,
}

impl ButtonBehaviour {
    pub const ALL: [ButtonBehaviour; 4] = [
        ButtonBehaviour::None,
        ButtonBehaviour::Invert,
        ButtonBehaviour::Outline,
        ButtonBehaviour::Push,
    ];

    fn name(self) -> &'static str {
        match self {
            ButtonBehaviour::None => "N",
            ButtonBehaviour::Invert => "I",
            ButtonBehaviour::Outline => "O",
            ButtonBehaviour::Push => "P",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ButtonBehaviour::None => "none",
            ButtonBehaviour::Invert => "invert",
            ButtonBehaviour::Outline => "outline",
            ButtonBehaviour::Push => "push",
        }
    }
}

impl Document {
    fn push_button_widgets(&self, field: ObjectId) -> Result<Vec<ObjectId>> {
        let f = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == field)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        if f.kind != FieldKind::PushButton {
            return Err(Error::Invalid("the field is not a push button".into()));
        }
        Ok(self.field_widgets(field))
    }

    /// Give a push button a picture for one of its states, or take it away
    /// (`None`), and draw the button again (one undo step).
    pub fn set_button_icon(
        &mut self,
        field: ObjectId,
        state: ButtonState,
        picture: Option<Vec<u8>>,
    ) -> Result<()> {
        let widgets = self.push_button_widgets(field)?;
        let icon = picture.map(image_object).transpose()?;
        self.begin_edit();
        let icon_id = match icon {
            Some(img) => {
                let (w, h) = (img.width, img.height);
                let image = self.add_image_object(img);
                // A form of the picture's own size: the appearance scales it to fit.
                Some(self.inner.add_object(Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Form",
                        "BBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                        "Resources" => dictionary! { "XObject" => dictionary! { "FoliPic" => image } },
                    },
                    format!("q {w} 0 0 {h} 0 0 cm /FoliPic Do Q").into_bytes(),
                )))
            }
            None => None,
        };
        for wid in widgets {
            let wd = self.inner.get_dictionary_mut(wid)?;
            let mut mk = wd
                .get(b"MK")
                .ok()
                .and_then(|m| m.as_dict().ok())
                .cloned()
                .unwrap_or_default();
            match icon_id {
                Some(id) => mk.set(state.key(), Object::Reference(id)),
                None => {
                    mk.remove(state.key().as_bytes());
                }
            }
            // The caption below the picture when there is one, else the picture alone.
            let has_icon = ["I", "IX", "RI"].iter().any(|k| mk.has(k.as_bytes()));
            if has_icon {
                let caption = mk
                    .get(b"CA")
                    .ok()
                    .and_then(|c| c.as_str().ok())
                    .is_some_and(|c| !c.is_empty());
                mk.set("TP", if caption { 2 } else { 1 });
            } else {
                mk.remove(b"TP");
            }
            wd.set("MK", mk);
            self.refresh_widget(wid)?;
        }
        self.commit()
    }

    /// Which states of a push button have a picture (up, down, rollover).
    pub fn button_icons(&self, field: ObjectId) -> [bool; 3] {
        let Some(wid) = self.field_widgets(field).first().copied() else {
            return [false; 3];
        };
        let mk = self
            .inner
            .get_dictionary(wid)
            .ok()
            .and_then(|d| d.get(b"MK").ok())
            .and_then(|m| m.as_dict().ok());
        ButtonState::ALL.map(|s| mk.is_some_and(|m| m.has(s.key().as_bytes())))
    }

    /// How a push button reacts to a click.
    pub fn button_behaviour(&self, field: ObjectId) -> ButtonBehaviour {
        let h = self
            .field_widgets(field)
            .first()
            .and_then(|w| self.inner.get_dictionary(*w).ok())
            .and_then(|d| d.get(b"H").ok())
            .and_then(|h| h.as_name().ok())
            .map(|n| n.to_vec());
        match h.as_deref() {
            Some(b"N") => ButtonBehaviour::None,
            Some(b"O") => ButtonBehaviour::Outline,
            Some(b"P") => ButtonBehaviour::Push,
            _ => ButtonBehaviour::Invert,
        }
    }

    /// Set how a push button reacts to a click (one undo step).
    pub fn set_button_behaviour(&mut self, field: ObjectId, behaviour: ButtonBehaviour) -> Result<()> {
        let widgets = self.push_button_widgets(field)?;
        self.begin_edit();
        for wid in widgets {
            self.inner
                .get_dictionary_mut(wid)?
                .set("H", Object::Name(behaviour.name().as_bytes().to_vec()));
        }
        self.commit()
    }
}
