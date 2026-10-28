//! Field calculations: a field whose value is the sum, product, average,
//! smallest or largest value of other fields. It is written as the standard
//! `AFSimple_Calculate` calculate action (`/AA /C`), so other viewers compute
//! it too, and computed here whenever a value changes, in the calculation
//! order of the form (`/CO`). Calculate actions written in other ways are
//! kept and listed, but not run.

use lopdf::{Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::forms::{FieldKind, FormField};
use crate::util::{deref, text_of};

/// What a simple calculation does with the values of its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcOp {
    Sum,
    Product,
    Average,
    Min,
    Max,
}

impl CalcOp {
    pub const ALL: [CalcOp; 5] = [
        CalcOp::Sum,
        CalcOp::Product,
        CalcOp::Average,
        CalcOp::Min,
        CalcOp::Max,
    ];

    /// The name `AFSimple_Calculate` uses.
    pub fn id(self) -> &'static str {
        match self {
            CalcOp::Sum => "SUM",
            CalcOp::Product => "PRD",
            CalcOp::Average => "AVG",
            CalcOp::Min => "MIN",
            CalcOp::Max => "MAX",
        }
    }

    pub fn from_id(id: &str) -> Option<CalcOp> {
        CalcOp::ALL.into_iter().find(|o| o.id().eq_ignore_ascii_case(id))
    }

    fn apply(self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        match self {
            CalcOp::Sum => values.iter().sum(),
            CalcOp::Product => values.iter().product(),
            CalcOp::Average => values.iter().sum::<f64>() / values.len() as f64,
            CalcOp::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
            CalcOp::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

/// A simple calculation: an operation over the values of fields named by
/// their full names (a name of a group stands for every field in it).
#[derive(Debug, Clone, PartialEq)]
pub struct Calculation {
    pub op: CalcOp,
    pub fields: Vec<String>,
}

impl Calculation {
    /// The calculate script: `AFSimple_Calculate("SUM", new Array("a", "b"));`.
    pub fn script(&self) -> String {
        let names: Vec<String> = self
            .fields
            .iter()
            .map(|n| format!("\"{}\"", n.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();
        format!(
            "AFSimple_Calculate(\"{}\", new Array({}));",
            self.op.id(),
            names.join(", ")
        )
    }

    /// Read a calculate script written with `AFSimple_Calculate`, its names
    /// in `new Array(...)` or `[...]`, in single or double quotes.
    pub fn parse(script: &str) -> Option<Calculation> {
        let s = script.trim();
        let rest = s
            .strip_prefix("AFSimple_Calculate")?
            .trim_start()
            .strip_prefix('(')?;
        let mut strings = Vec::new();
        let mut chars = rest.chars().peekable();
        let mut depth = 1;
        while let Some(c) = chars.next() {
            match c {
                '"' | '\'' => {
                    let mut out = String::new();
                    while let Some(d) = chars.next() {
                        match d {
                            '\\' => {
                                if let Some(e) = chars.next() {
                                    out.push(e);
                                }
                            }
                            q if q == c => break,
                            other => out.push(other),
                        }
                    }
                    strings.push(out);
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        let (op, fields) = strings.split_first()?;
        Some(Calculation {
            op: CalcOp::from_id(op)?,
            fields: fields
                .iter()
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect(),
        })
    }
}

/// The number a field value holds: spaces and thousands separators left
/// out, a comma read as the decimal point when there is no point; anything
/// else counts as 0.
pub fn number_of(value: &str) -> f64 {
    let v: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let v = if v.contains('.') {
        v.replace(',', "")
    } else {
        v.replace(',', ".")
    };
    let v = v.trim_start_matches(['$', '€', '£']).trim_end_matches(['%', '€']);
    v.parse::<f64>().ok().filter(|n| n.is_finite()).unwrap_or(0.0)
}

/// A calculated number as a field value: no exponent, at most ten decimals,
/// no trailing zeros.
pub fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return String::new();
    }
    let s = format!("{n:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

impl Document {
    /// The calculate script of a field (`/AA /C /JS`).
    pub fn calculate_script(&self, id: ObjectId) -> Option<String> {
        self.field_script(id, b"C")
    }

    /// The script a field runs on `trigger` (`/AA /<trigger> /JS`: `C`
    /// calculate, `F` format, `V` validate, `K` keystroke), looked for on
    /// `id` (a field or a widget) and on the fields above it.
    pub fn field_script(&self, id: ObjectId, trigger: &[u8]) -> Option<String> {
        let doc = &self.inner;
        let mut cur = Some(id);
        for _ in 0..32 {
            let d = doc.get_dictionary(cur?).ok()?;
            let action = d
                .get(b"AA")
                .ok()
                .and_then(|aa| deref(doc, aa).as_dict().ok())
                .and_then(|aa| aa.get(trigger).ok())
                .and_then(|a| deref(doc, a).as_dict().ok());
            if let Some(js) = action.and_then(|a| a.get(b"JS").ok()) {
                return match deref(doc, js) {
                    Object::Stream(s) => s
                        .decompressed_content()
                        .ok()
                        .or_else(|| Some(s.content.clone()))
                        .map(|b| String::from_utf8_lossy(&b).into_owned()),
                    other => text_of(doc, other),
                };
            }
            cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        None
    }

    /// Run the script of `trigger` of field `id` with the value `value`: the
    /// value it leaves and whether it accepts it, with its alerts; `None`
    /// when the field has no such script.
    pub fn run_field_script(
        &self,
        id: ObjectId,
        trigger: &[u8],
        value: &str,
    ) -> Option<crate::js::ScriptRun> {
        let code = self.field_script(id, trigger)?;
        let fields: Vec<(String, String)> = self
            .form_fields()
            .into_iter()
            .map(|f| (f.name, f.value))
            .collect();
        let name = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == id || f.widgets.contains(&id))
            .map(|f| f.name)
            .unwrap_or_default();
        let ctx = crate::js::field_context(&fields);
        let event =
            serde_json::json!({ "value": value, "target": name, "name": String::from_utf8_lossy(trigger) });
        Some(crate::js::run_script(&code, &ctx, Some(&event)))
    }

    /// The simple calculation of a field, if it has one.
    pub fn field_calculation(&self, id: ObjectId) -> Option<Calculation> {
        self.calculate_script(id).and_then(|s| Calculation::parse(&s))
    }

    /// Fields with a calculate action, in the order they are calculated:
    /// those of `/CO` first, then the others in the order of the form.
    pub fn calculated_fields(&self) -> Vec<FormField> {
        let fields = self.form_fields();
        let order = self.calculation_order();
        let mut out: Vec<FormField> = order
            .iter()
            .filter_map(|id| fields.iter().find(|f| f.id == *id).cloned())
            .collect();
        for f in &fields {
            if !order.contains(&f.id) && self.calculate_script(f.id).is_some() {
                out.push(f.clone());
            }
        }
        out
    }

    fn set_co(&mut self, order: Vec<ObjectId>) -> Result<()> {
        let form = self.ensure_acroform()?;
        let dict = self.inner.get_dictionary_mut(form)?;
        if order.is_empty() {
            dict.remove(b"CO");
        } else {
            dict.set("CO", order.into_iter().map(Object::Reference).collect::<Vec<_>>());
        }
        Ok(())
    }

    /// Calculate the field `id` with `calc` (or no longer, with `None`); its
    /// value is calculated at once. One undo step.
    pub fn set_field_calculation(&mut self, id: ObjectId, calc: Option<&Calculation>) -> Result<()> {
        let field = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        if !matches!(field.kind, FieldKind::Text | FieldKind::ComboBox) {
            return Err(Error::Invalid(
                "only text fields and combo boxes can be calculated".into(),
            ));
        }
        if let Some(c) = calc {
            if c.fields.iter().any(|n| covers(n, &field.name)) {
                return Err(Error::Invalid(format!(
                    "the field \"{}\" cannot use its own value",
                    field.name
                )));
            }
        }
        let mut order: Vec<ObjectId> = self.calculated_fields().iter().map(|f| f.id).collect();
        let mut aa = self
            .inner
            .get_dictionary(id)?
            .get(b"AA")
            .ok()
            .and_then(|a| deref(&self.inner, a).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        self.begin_edit();
        match calc {
            Some(c) => {
                aa.set(
                    "C",
                    dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(&c.script()) },
                );
                if !order.contains(&id) {
                    order.push(id);
                }
            }
            None => {
                aa.remove(b"C");
                order.retain(|o| *o != id);
            }
        }
        let dict = self.inner.get_dictionary_mut(id)?;
        if aa.is_empty() {
            dict.remove(b"AA");
        } else {
            dict.set("AA", aa);
        }
        self.set_co(order)?;
        self.recalculate()?;
        self.commit()
    }

    /// Set (or remove, with `None`) the script field `id` runs on `trigger`:
    /// `C` calculate, `F` format, `V` validate, `K` keystroke. The field is
    /// calculated and drawn again with it. One undo step.
    pub fn set_field_script(&mut self, id: ObjectId, trigger: &str, script: Option<&str>) -> Result<()> {
        if !matches!(trigger, "C" | "F" | "V" | "K") {
            return Err(Error::Invalid(format!("unknown trigger {trigger:?}")));
        }
        let field = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        let mut order: Vec<ObjectId> = self.calculated_fields().iter().map(|f| f.id).collect();
        let mut aa = self
            .inner
            .get_dictionary(id)?
            .get(b"AA")
            .ok()
            .and_then(|a| deref(&self.inner, a).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        self.begin_edit();
        match script.filter(|s| !s.trim().is_empty()) {
            Some(js) => {
                aa.set(
                    trigger,
                    dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(js) },
                );
                if trigger == "C" && !order.contains(&id) {
                    order.push(id);
                }
            }
            None => {
                aa.remove(trigger.as_bytes());
                if trigger == "C" {
                    order.retain(|o| *o != id);
                }
            }
        }
        let dict = self.inner.get_dictionary_mut(id)?;
        if aa.is_empty() {
            dict.remove(b"AA");
        } else {
            dict.set("AA", aa);
        }
        if trigger == "C" {
            self.set_co(order)?;
        }
        self.recalculate()?;
        for w in field.widgets {
            self.refresh_widget(w)?;
        }
        self.commit()
    }

    /// Set the order in which the calculated fields are calculated. One undo step.
    pub fn set_calculation_order(&mut self, order: &[ObjectId]) -> Result<()> {
        self.begin_edit();
        self.set_co(order.to_vec())?;
        self.recalculate()?;
        self.commit()
    }

    /// Calculate every field with a simple calculation, in calculation
    /// order, each seeing the values calculated before it. Part of the edit
    /// under way (no undo step of its own). Returns how many values changed.
    pub(crate) fn recalculate(&mut self) -> Result<usize> {
        let calculated = self.calculated_fields();
        if calculated.is_empty() {
            return Ok(0);
        }
        let mut fields = self.form_fields();
        let mut changed = 0;
        for target in calculated {
            let Some(calc) = self.field_calculation(target.id) else {
                // A calculation of its own: the script gives the value.
                let current = fields
                    .iter()
                    .find(|f| f.id == target.id)
                    .map(|f| f.value.clone())
                    .unwrap_or_default();
                let Some(run) = self.run_field_script(target.id, b"C", &current) else {
                    continue;
                };
                if let (true, Some(value)) = (run.ok, run.event_value) {
                    if let Some(f) = fields.iter_mut().find(|f| f.id == target.id) {
                        if f.value != value {
                            self.write_field_value(f, &value)?;
                            f.value = value;
                            changed += 1;
                        }
                    }
                }
                continue;
            };
            let values: Vec<f64> = fields
                .iter()
                .filter(|f| calc.fields.iter().any(|n| covers(n, &f.name)))
                .filter(|f| f.kind != FieldKind::PushButton && f.kind != FieldKind::Signature)
                .map(|f| number_of(&f.value))
                .collect();
            let value = format_number(calc.op.apply(&values));
            if let Some(f) = fields.iter_mut().find(|f| f.id == target.id) {
                if f.value != value {
                    self.write_field_value(f, &value)?;
                    f.value = value;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }
}

/// Whether the name `name` in a calculation stands for the field `field`
/// (the field itself, or a group it is in).
fn covers(name: &str, field: &str) -> bool {
    field == name || field.strip_prefix(name).is_some_and(|rest| rest.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_read_and_written() {
        let c = Calculation {
            op: CalcOp::Sum,
            fields: vec!["price".into(), "tax \"x\"".into()],
        };
        assert_eq!(Calculation::parse(&c.script()), Some(c));
        let other = Calculation::parse("AFSimple_Calculate('avg', ['a.b', 'c']);").unwrap();
        assert_eq!(other.op, CalcOp::Average);
        assert_eq!(other.fields, ["a.b", "c"]);
        assert_eq!(Calculation::parse("event.value = 1;"), None);
    }

    #[test]
    fn numbers_and_names() {
        assert_eq!(number_of("1,234.50"), 1234.5);
        assert_eq!(number_of("12,5"), 12.5);
        assert_eq!(number_of(" € 3 "), 3.0);
        assert_eq!(number_of("abc"), 0.0);
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(12.0), "12");
        assert_eq!(format_number(-0.0), "0");
        assert!(covers("row", "row.1") && covers("row", "row") && !covers("row", "rows"));
        assert_eq!(CalcOp::Min.apply(&[3.0, -1.0, 2.0]), -1.0);
        assert_eq!(CalcOp::Average.apply(&[]), 0.0);
    }
}
