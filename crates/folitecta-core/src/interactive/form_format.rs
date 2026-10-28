//! Formats and checks of text fields chosen instead of written: numbers,
//! percentages, dates, times and the special formats (postal codes, phone
//! numbers), and the range a number must be in. They are kept as the
//! standard scripts of forms (`/AA /F` format, `/K` keystroke, `/V`
//! validate), so other readers apply them too; scripts written by hand are
//! recognised as such and left as they are.

use lopdf::{ObjectId, dictionary};

use crate::document::Document;
use crate::error::Result;
use crate::util::deref;

/// How a text field shows its value and which values it takes.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FieldFormat {
    /// The value as typed.
    #[default]
    None,
    /// A number: `decimals`, the separator style (0 `1,234.56`, 1
    /// `1234.56`, 2 `1.234,56`, 3 `1234,56`, 4 `1'234.56`), negative numbers
    /// in parentheses or not, and a currency symbol before or after.
    Number {
        decimals: u8,
        separators: u8,
        parentheses: bool,
        currency: String,
        before: bool,
    },
    /// A percentage (the value 0.25 shows as 25%).
    Percent { decimals: u8, separators: u8 },
    /// A date in a `util.printd` format (`dd/mm/yyyy`, `d mmmm yyyy`, …).
    Date(String),
    /// A time in the same kind of format (`HH:MM`, `h:MM tt`, …).
    Time(String),
    /// 0 postal code, 1 postal code + 4, 2 phone number, 3 social
    /// security number.
    Special(u8),
    /// Scripts written by hand.
    Custom,
}

/// Date formats offered for a date field.
pub const DATE_FORMATS: &[&str] = &[
    "dd/mm/yyyy",
    "mm/dd/yyyy",
    "yyyy-mm-dd",
    "d mmmm yyyy",
    "mmmm d, yyyy",
    "dd-mmm-yyyy",
    "dd.mm.yyyy",
    "mm/yyyy",
];

/// Time formats offered for a time field.
pub const TIME_FORMATS: &[&str] = &["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];

/// What a value must be to be taken, besides its format.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FieldCheck {
    #[default]
    None,
    /// A number between `min` and `max` (either may be open).
    Range { min: Option<f64>, max: Option<f64> },
    /// A script written by hand.
    Custom,
}

/// The date formats of `AFDate_Format(n)` (the older scripts number them).
const NUMBERED_DATES: &[&str] = &[
    "m/d",
    "m/d/yy",
    "mm/dd/yy",
    "mm/yy",
    "d-mmm",
    "d-mmm-yy",
    "dd-mmm-yy",
    "yy-mm-dd",
    "mmm-yy",
    "mmmm-yy",
    "mmm d, yyyy",
    "mmmm d, yyyy",
    "m/d/yy h:MM tt",
    "m/d/yy HH:MM",
];

/// The time formats of `AFTime_Format(n)`.
const NUMBERED_TIMES: &[&str] = &["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];

/// The arguments of `script` when it is one call of `func` and nothing
/// else (strings without their quotes).
fn call_args(script: &str, func: &str) -> Option<Vec<String>> {
    let s = script.trim().trim_end_matches(';').trim();
    let inner = s
        .strip_prefix(func)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let mut args = Vec::new();
    // The text of the argument being read; a string keeps its spaces.
    let mut cur = String::new();
    let mut string: Option<String> = None;
    let (mut quote, mut escaped) = (None, false);
    let mut take = |cur: &mut String, string: &mut Option<String>| {
        args.push(string.take().unwrap_or_else(|| cur.trim().to_string()));
        cur.clear();
    };
    for c in inner.chars() {
        if let Some(q) = quote {
            let s = string.get_or_insert_with(String::new);
            if escaped {
                s.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            } else {
                s.push(c);
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                string.get_or_insert_with(String::new);
            }
            ',' => take(&mut cur, &mut string),
            '(' | ')' | ';' => return None,
            _ => cur.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !inner.trim().is_empty() {
        take(&mut cur, &mut string);
    }
    Some(args)
}

fn int(args: &[String], i: usize) -> u8 {
    args.get(i)
        .and_then(|a| a.parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 255.0) as u8
}

fn yes(args: &[String], i: usize) -> bool {
    args.get(i)
        .is_some_and(|a| a == "true" || a.parse::<f64>().is_ok_and(|n| n != 0.0))
}

/// A string as a script writes it.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

impl FieldFormat {
    /// The format the scripts of a field (`/AA /F` and `/AA /K`) stand for.
    pub fn from_scripts(format: Option<&str>, keystroke: Option<&str>) -> FieldFormat {
        let Some(f) = format.filter(|f| !f.trim().is_empty()) else {
            return if keystroke.is_some_and(|k| !k.trim().is_empty()) {
                FieldFormat::Custom
            } else {
                FieldFormat::None
            };
        };
        let found = if let Some(a) = call_args(f, "AFNumber_Format") {
            Some((
                FieldFormat::Number {
                    decimals: int(&a, 0),
                    separators: int(&a, 1),
                    parentheses: int(&a, 2) >= 2,
                    currency: a.get(4).cloned().unwrap_or_default(),
                    before: yes(&a, 5),
                },
                "AFNumber_Keystroke",
            ))
        } else if let Some(a) = call_args(f, "AFPercent_Format") {
            Some((
                FieldFormat::Percent {
                    decimals: int(&a, 0),
                    separators: int(&a, 1),
                },
                "AFPercent_Keystroke",
            ))
        } else if let Some(a) = call_args(f, "AFDate_FormatEx") {
            a.first()
                .map(|s| (FieldFormat::Date(s.clone()), "AFDate_Keystroke"))
        } else if let Some(a) = call_args(f, "AFDate_Format") {
            NUMBERED_DATES
                .get(int(&a, 0) as usize)
                .map(|s| (FieldFormat::Date(s.to_string()), "AFDate_Keystroke"))
        } else if let Some(a) = call_args(f, "AFTime_FormatEx") {
            a.first()
                .map(|s| (FieldFormat::Time(s.clone()), "AFTime_Keystroke"))
        } else if let Some(a) = call_args(f, "AFTime_Format") {
            NUMBERED_TIMES
                .get(int(&a, 0) as usize)
                .map(|s| (FieldFormat::Time(s.to_string()), "AFTime_Keystroke"))
        } else {
            call_args(f, "AFSpecial_Format")
                .map(|a| (FieldFormat::Special(int(&a, 0).min(3)), "AFSpecial_Keystroke"))
        };
        match found {
            // The keystroke script, when there is one, must be of the same kind.
            Some((fmt, k)) if keystroke.is_none_or(|ks| ks.trim().is_empty() || ks.trim().starts_with(k)) => {
                fmt
            }
            _ => FieldFormat::Custom,
        }
    }

    /// The format and keystroke scripts of this format (`None` for none;
    /// not called for [`FieldFormat::Custom`]).
    pub fn scripts(&self) -> Option<(String, String)> {
        let pair = |f: String, k: String| Some((format!("{f};"), format!("{k};")));
        match self {
            FieldFormat::None | FieldFormat::Custom => None,
            FieldFormat::Number {
                decimals,
                separators,
                parentheses,
                currency,
                before,
            } => {
                let args = format!(
                    "{decimals}, {separators}, {}, 0, {}, {before}",
                    if *parentheses { 2 } else { 0 },
                    quoted(currency)
                );
                pair(
                    format!("AFNumber_Format({args})"),
                    format!("AFNumber_Keystroke({args})"),
                )
            }
            FieldFormat::Percent { decimals, separators } => pair(
                format!("AFPercent_Format({decimals}, {separators})"),
                format!("AFPercent_Keystroke({decimals}, {separators})"),
            ),
            FieldFormat::Date(f) => pair(
                format!("AFDate_FormatEx({})", quoted(f)),
                format!("AFDate_KeystrokeEx({})", quoted(f)),
            ),
            FieldFormat::Time(f) => pair(
                format!("AFTime_FormatEx({})", quoted(f)),
                format!("AFTime_KeystrokeEx({})", quoted(f)),
            ),
            FieldFormat::Special(n) => pair(
                format!("AFSpecial_Format({n})"),
                format!("AFSpecial_Keystroke({n})"),
            ),
        }
    }
}

impl FieldCheck {
    /// The check the validate script of a field (`/AA /V`) stands for.
    pub fn from_script(validate: Option<&str>) -> FieldCheck {
        let Some(v) = validate.filter(|v| !v.trim().is_empty()) else {
            return FieldCheck::None;
        };
        match call_args(v, "AFRange_Validate") {
            Some(a) if a.len() == 4 => {
                let num = |i: usize| a.get(i).and_then(|n| n.parse::<f64>().ok());
                FieldCheck::Range {
                    min: if yes(&a, 0) { num(1) } else { None },
                    max: if yes(&a, 2) { num(3) } else { None },
                }
            }
            _ => FieldCheck::Custom,
        }
    }

    /// The validate script of this check (`None` for none; not called for
    /// [`FieldCheck::Custom`]).
    pub fn script(&self) -> Option<String> {
        match self {
            FieldCheck::Range { min, max } if min.is_some() || max.is_some() => Some(format!(
                "AFRange_Validate({}, {}, {}, {});",
                min.is_some(),
                min.unwrap_or(0.0),
                max.is_some(),
                max.unwrap_or(0.0)
            )),
            _ => None,
        }
    }
}

/// The date `year`-`month`-`day` written in the date format `fmt`.
pub fn print_date(fmt: &str, year: i32, month: u32, day: u32) -> String {
    let code = format!(
        "util.printd({}, new Date({year}, {}, {day}))",
        quoted(fmt),
        month.saturating_sub(1)
    );
    crate::js::run_script(&code, &crate::js::field_context(&[]), None)
        .result
        .unwrap_or_default()
}

/// The date `text` written in the format `fmt` stands for (year, month,
/// day), when it is a real date.
pub fn scan_date(fmt: &str, text: &str) -> Option<(i32, u32, u32)> {
    let code = format!(
        "const t = util.scand({}, {}); t ? [t.getFullYear(), t.getMonth() + 1, t.getDate()].join('-') : ''",
        quoted(fmt),
        quoted(text)
    );
    let out = crate::js::run_script(&code, &crate::js::field_context(&[]), None).result?;
    let mut p = out.split('-').filter_map(|n| n.parse::<i64>().ok());
    Some((p.next()? as i32, p.next()? as u32, p.next()? as u32))
}

impl Document {
    /// The format of a text field (from its format and keystroke scripts).
    pub fn field_format(&self, id: ObjectId) -> FieldFormat {
        FieldFormat::from_scripts(
            self.field_script(id, b"F").as_deref(),
            self.field_script(id, b"K").as_deref(),
        )
    }

    /// The check of a text field (from its validate script).
    pub fn field_check(&self, id: ObjectId) -> FieldCheck {
        FieldCheck::from_script(self.field_script(id, b"V").as_deref())
    }

    /// Write `format` and `check` as the scripts of field `id`, without an
    /// undo step of its own; a custom one, or one already there, changes
    /// nothing. The widgets are not redrawn here.
    pub(crate) fn write_field_format(
        &mut self,
        id: ObjectId,
        format: &FieldFormat,
        check: &FieldCheck,
    ) -> Result<()> {
        if *format != FieldFormat::Custom && *format != self.field_format(id) {
            let (f, k) = format.scripts().unzip();
            self.set_aa_script(id, b"F", f)?;
            self.set_aa_script(id, b"K", k)?;
        }
        if *check != FieldCheck::Custom && *check != self.field_check(id) {
            self.set_aa_script(id, b"V", check.script())?;
        }
        Ok(())
    }

    /// Set or remove the script of `trigger` in the `/AA` of `id`.
    fn set_aa_script(&mut self, id: ObjectId, trigger: &[u8], script: Option<String>) -> Result<()> {
        let mut aa = self
            .inner
            .get_dictionary(id)?
            .get(b"AA")
            .ok()
            .and_then(|a| deref(&self.inner, a).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        match script {
            Some(js) => aa.set(
                trigger.to_vec(),
                dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(&js) },
            ),
            None => {
                aa.remove(trigger);
            }
        }
        let d = self.inner.get_dictionary_mut(id)?;
        if aa.is_empty() {
            d.remove(b"AA");
        } else {
            d.set("AA", aa);
        }
        Ok(())
    }
}
