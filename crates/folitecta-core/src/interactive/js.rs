//! The JavaScript of documents (FOL-L034), run by Boa in a sandbox: a
//! fresh engine for every run, with nothing of the system in reach (no
//! network, files, databases or programs) and limits on loops and
//! recursion, so a script that never ends is stopped. Scripts see the
//! documented API of [`API`]: the document (`this`: pages, fields,
//! information), `event`, `app.alert`, `console`, `util` and the `AF…`
//! functions of form fields (calculations, number, percent and special
//! formats, keystrokes, ranges). What a script asks to change comes back
//! as operations applied afterwards by the caller.

use boa_engine::{Context, Source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The functions and objects scripts can use, for the documentation.
pub const API: &[&str] = &[
    "this.numPages",
    "this.pageNum",
    "this.numFields",
    "this.getField(name).value / valueAsString / name / type / readonly / required",
    "this.getNthFieldName(n)",
    "this.info.Title / Author / Subject / Keywords, this.title / author / subject / keywords",
    "this.documentFileName, this.path, this.filesize, this.dirty",
    "this.resetForm()",
    "event.value, event.change, event.rc, event.target, event.name, event.willCommit",
    "app.alert(message), app.beep(), app.viewerType, app.platform, app.language",
    "console.println(text), console.log(…), console.clear()",
    "util.printf(format, …) with %s %d %f %.nf, util.printd(format, date)",
    "AFSimple_Calculate(\"SUM\" | \"PRD\" | \"AVG\" | \"MIN\" | \"MAX\", fields)",
    "AFNumber_Format(decimals, separators, negative, currency, symbol, before), AFNumber_Keystroke(…)",
    "AFPercent_Format(decimals, separators), AFPercent_Keystroke(…)",
    "AFSpecial_Format(0 zip | 1 zip+4 | 2 phone | 3 social security), AFSpecial_Keystroke(…)",
    "AFRange_Validate(above?, lowest, below?, highest)",
];

/// How much a script may do before it is stopped.
const LOOP_LIMIT: u64 = 5_000_000;
const RECURSION_LIMIT: usize = 400;

/// What a run gave back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptRun {
    pub ok: bool,
    pub error: Option<String>,
    /// The value of the last expression, as text.
    pub result: Option<String>,
    /// Console and alert lines: kind (`out`, `alert`) and text.
    pub out: Vec<(String, String)>,
    /// The changes asked for: `{op: "field", name, value}`, `{op: "page",
    /// page}`, `{op: "info", key, value}`, `{op: "reset"}`, `{op: "clear"}`.
    pub ops: Vec<Value>,
    /// The event after the script: its value and whether it was accepted.
    pub event_value: Option<String>,
    pub event_rc: bool,
}

/// The API, then the script; the outcome as JSON text.
const PRELUDE: &str = r#"
(function () {
  const out = [], ops = [];
  const str = (v) => { if (typeof v === 'string') return v; try { const j = JSON.stringify(v); return j === undefined ? String(v) : j; } catch (_) { return String(v); } };
  const console = {
    println: (s) => out.push(['out', String(s)]),
    log: (...a) => out.push(['out', a.map(str).join(' ')]),
    clear: () => ops.push({ op: 'clear' }),
    show() {}, hide() {},
  };
  const app = {
    alert: (m) => { const text = m && typeof m === 'object' ? m.cMsg : m; out.push(['alert', String(text)]); return 1; },
    beep() {}, response: () => null,
    viewerType: 'Folitecta', viewerVariation: 'Full', viewerVersion: ctx.version, platform: ctx.platform, language: ctx.lang,
  };
  const fields = ctx.fields || {};
  const names = Object.keys(fields);
  const field = (name) => {
    const f = fields[name];
    if (!f) return null;
    return {
      name, type: f.type, readonly: f.readonly, required: f.required,
      get value() { return f.value; },
      set value(v) { f.value = v == null ? '' : String(v); ops.push({ op: 'field', name, value: f.value }); },
      get valueAsString() { return String(f.value); },
    };
  };
  const infoData = Object.assign({}, ctx.info || {});
  const info = {};
  for (const k of ['Title', 'Author', 'Subject', 'Keywords', 'Creator', 'Producer']) {
    Object.defineProperty(info, k, {
      enumerable: true,
      get() { return infoData[k] || ''; },
      set(v) { infoData[k] = String(v); ops.push({ op: 'info', key: k, value: String(v) }); },
    });
  }
  const doc = {
    numPages: ctx.numPages || 1,
    get pageNum() { return ctx.page || 0; },
    set pageNum(n) { const p = Math.max(0, Math.min((ctx.numPages || 1) - 1, Math.trunc(Number(n) || 0))); ctx.page = p; ops.push({ op: 'page', page: p }); },
    numFields: names.length,
    getField: field,
    getNthFieldName: (i) => names[i] ?? null,
    info,
    get title() { return info.Title; }, set title(v) { info.Title = v; },
    get author() { return info.Author; }, set author(v) { info.Author = v; },
    get subject() { return info.Subject; }, set subject(v) { info.Subject = v; },
    get keywords() { return info.Keywords; }, set keywords(v) { info.Keywords = v; },
    documentFileName: ctx.fileName || '', path: ctx.path || '', filesize: ctx.size || 0, dirty: !!ctx.dirty,
    resetForm() { ops.push({ op: 'reset' }); },
  };
  const pad = (n, w) => { let s = String(n); while (s.length < w) s = '0' + s; return s; };
  const util = {
    printf(fmt, ...args) {
      let i = 0;
      return String(fmt).replace(/%(\.(\d+))?([sdf%])/g, (m, _p, digits, k) => {
        if (k === '%') return '%';
        const v = args[i++];
        if (k === 's') return String(v);
        if (k === 'd') return String(Math.trunc(Number(v)));
        return Number(v).toFixed(digits === undefined ? 6 : Number(digits));
      });
    },
    printd(fmt, d) {
      const t = d instanceof Date ? d : new Date(d);
      return String(fmt)
        .replace(/yyyy/g, pad(t.getFullYear(), 4)).replace(/mm/g, pad(t.getMonth() + 1, 2))
        .replace(/dd/g, pad(t.getDate(), 2)).replace(/HH/g, pad(t.getHours(), 2))
        .replace(/MM/g, pad(t.getMinutes(), 2)).replace(/ss/g, pad(t.getSeconds(), 2));
    },
    scand: (fmt, s) => new Date(s),
  };
  const event = Object.assign({ value: '', change: '', rc: true, willCommit: true, name: '', target: null }, evt || {});
  if (typeof event.target === 'string') event.target = field(event.target);

  // Numbers as forms write them: spaces and thousands separators left out,
  // a comma as the decimal point when there is no point.
  const toNumber = (v) => {
    let s = String(v == null ? '' : v).replace(/\s/g, '');
    s = s.indexOf('.') >= 0 ? s.replace(/,/g, '') : s.replace(/,/g, '.');
    s = s.replace(/^[^0-9+\-.]+/, '').replace(/[^0-9.]+$/, '');
    const n = parseFloat(s);
    return isFinite(n) ? n : 0;
  };
  const group = (digits, sep) => digits.replace(/\B(?=(\d{3})+(?!\d))/g, sep);
  // Separator styles: 0 1,234.56  1 1234.56  2 1.234,56  3 1234,56  4 1'234.56
  const formatNumber = (n, dec, sepStyle) => {
    const fixed = Math.abs(n).toFixed(Math.max(0, Math.min(10, dec | 0)));
    let [whole, frac] = fixed.split('.');
    const styles = [[',', '.'], ['', '.'], ['.', ','], ['', ','], ["'", '.']];
    const [thousands, point] = styles[sepStyle] || styles[0];
    if (thousands) whole = group(whole, thousands);
    return frac === undefined ? whole : whole + point + frac;
  };
  const AFSimple_Calculate = (op, list) => {
    const wanted = typeof list === 'string' ? list.split(',').map((s) => s.trim()) : Array.from(list || []);
    const values = [];
    for (const n of wanted) {
      for (const k of names) {
        if (k === n || k.startsWith(n + '.')) values.push(toNumber(fields[k].value));
      }
    }
    let r = 0;
    switch (String(op).toUpperCase()) {
      case 'SUM': r = values.reduce((a, b) => a + b, 0); break;
      case 'PRD': r = values.length ? values.reduce((a, b) => a * b, 1) : 0; break;
      case 'AVG': r = values.length ? values.reduce((a, b) => a + b, 0) / values.length : 0; break;
      case 'MIN': r = values.length ? Math.min(...values) : 0; break;
      case 'MAX': r = values.length ? Math.max(...values) : 0; break;
    }
    event.value = String(Math.round(r * 1e10) / 1e10);
  };
  const AFNumber_Format = (dec, sepStyle, negStyle, currStyle, strCurrency, bCurrencyPrepend) => {
    if (String(event.value).trim() === '') return;
    const n = toNumber(event.value);
    let s = formatNumber(n, dec, sepStyle);
    const cur = strCurrency || '';
    if (cur) s = bCurrencyPrepend ? cur + s : s + cur;
    if (n < 0) s = (negStyle === 2 || negStyle === 3) ? '(' + s + ')' : '-' + s;
    event.value = s;
  };
  const AFNumber_Keystroke = (dec, sepStyle) => {
    const ch = String(event.change || '');
    if (ch && !/^[0-9.,\-+\s'()]*$/.test(ch)) event.rc = false;
  };
  const AFPercent_Format = (dec, sepStyle) => {
    if (String(event.value).trim() === '') return;
    event.value = formatNumber(toNumber(event.value) * 100, dec, sepStyle) + '%';
  };
  const AFPercent_Keystroke = AFNumber_Keystroke;
  const AFSpecial_Format = (psf) => {
    const d = String(event.value).replace(/\D/g, '');
    if (!d) return;
    switch (Number(psf)) {
      case 0: event.value = d.slice(0, 5); break;
      case 1: event.value = d.slice(0, 5) + (d.length > 5 ? '-' + d.slice(5, 9) : ''); break;
      case 2: event.value = d.length > 7 ? '(' + d.slice(0, 3) + ') ' + d.slice(3, 6) + '-' + d.slice(6, 10) : d.slice(0, 3) + '-' + d.slice(3, 7); break;
      case 3: event.value = d.slice(0, 3) + '-' + d.slice(3, 5) + '-' + d.slice(5, 9); break;
    }
  };
  const AFSpecial_Keystroke = (psf) => {
    const ch = String(event.change || '');
    if (ch && /[^0-9\-()\s]/.test(ch)) event.rc = false;
  };
  const AFRange_Validate = (bGreater, nGreater, bLess, nLess) => {
    if (String(event.value).trim() === '') return;
    const n = toNumber(event.value);
    let msg = '';
    if (bGreater && bLess && (n < nGreater || n > nLess)) msg = 'The value must be at least ' + nGreater + ' and at most ' + nLess + '.';
    else if (bGreater && !bLess && n < nGreater) msg = 'The value must be at least ' + nGreater + '.';
    else if (bLess && !bGreater && n > nLess) msg = 'The value must be at most ' + nLess + '.';
    if (msg) { event.rc = false; app.alert(msg); }
  };

  let result, ok = true, error = null;
  try {
    result = (function (app, console, util, event) { return eval(code); }).call(doc, app, console, util, event);
  } catch (err) {
    ok = false;
    error = err && err.name ? err.name + ': ' + err.message : String(err);
  }
  return JSON.stringify({
    ok, error, result: result === undefined || result === null ? null : str(result), out, ops,
    event_value: event.value == null ? null : String(event.value), event_rc: event.rc !== false,
  });
})()
"#;

/// Run `code` with the document `ctx` (as the console builds it: `fields`,
/// `info`, `numPages`, `page`, …) and, for the scripts of fields, `event`.
pub fn run_script(code: &str, ctx: &Value, event: Option<&Value>) -> ScriptRun {
    let source = format!(
        "const code = {};\nconst ctx = {};\nconst evt = {};\n{PRELUDE}",
        serde_json::to_string(code).unwrap_or_else(|_| "\"\"".into()),
        ctx,
        event.cloned().unwrap_or(Value::Null),
    );
    let mut context = Context::default();
    context.runtime_limits_mut().set_loop_iteration_limit(LOOP_LIMIT);
    context.runtime_limits_mut().set_recursion_limit(RECURSION_LIMIT);
    let failed = |why: String| ScriptRun {
        ok: false,
        error: Some(why),
        event_rc: true,
        ..ScriptRun::default()
    };
    match context.eval(Source::from_bytes(source.as_bytes())) {
        Ok(v) => {
            let text = v
                .to_string(&mut context)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            serde_json::from_str(&text).unwrap_or_else(|e| failed(format!("the script gave no outcome: {e}")))
        }
        // Errors outside the guarded script: limits reached, syntax.
        Err(e) => failed(e.to_string()),
    }
}

/// A minimal context of one field and its value, for the scripts of fields.
pub fn field_context(fields: &[(String, String)]) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in fields {
        map.insert(
            name.clone(),
            json!({ "value": value, "type": "text", "readonly": false, "required": false }),
        );
    }
    json!({ "fields": map, "numPages": 1, "page": 0, "info": {}, "version": crate::VERSION, "platform": std::env::consts::OS, "lang": "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Value {
        field_context(&[("price".into(), "12.5".into()), ("qty".into(), "3".into())])
    }

    #[test]
    fn scripts_see_the_document_and_change_it_through_operations() {
        let r = run_script(
            "console.println('hello'); this.getField('qty').value = 4; app.alert('done'); 6 * 7",
            &ctx(),
            None,
        );
        assert!(r.ok, "{r:?}");
        assert_eq!(r.result.as_deref(), Some("42"));
        assert_eq!(
            r.out,
            vec![
                ("out".to_string(), "hello".to_string()),
                ("alert".into(), "done".into())
            ]
        );
        assert_eq!(r.ops, vec![json!({"op": "field", "name": "qty", "value": "4"})]);
    }

    #[test]
    fn errors_and_endless_scripts_stop() {
        let r = run_script("nonsense(", &ctx(), None);
        assert!(!r.ok && r.error.is_some());
        let r = run_script("while (true) {}", &ctx(), None);
        assert!(!r.ok, "an endless loop is stopped: {r:?}");
        let r = run_script("function f(n) { return f(n + 1); } f(0)", &ctx(), None);
        assert!(!r.ok, "endless recursion is stopped");
        // Nothing of the system is in reach.
        for code in [
            "typeof require",
            "typeof process",
            "typeof fetch",
            "typeof XMLHttpRequest",
        ] {
            let r = run_script(code, &ctx(), None);
            assert_eq!(r.result.as_deref(), Some("undefined"), "{code}");
        }
    }

    #[test]
    fn the_af_functions_of_fields() {
        let calc = run_script(
            "AFSimple_Calculate('PRD', ['price', 'qty'])",
            &ctx(),
            Some(&json!({"value": ""})),
        );
        assert_eq!(calc.event_value.as_deref(), Some("37.5"));
        let fmt = run_script(
            "AFNumber_Format(2, 0, 0, 0, '€ ', true)",
            &ctx(),
            Some(&json!({"value": "1234.5"})),
        );
        assert_eq!(fmt.event_value.as_deref(), Some("€ 1,234.50"));
        let it = run_script(
            "AFNumber_Format(2, 2, 0, 0, '', false)",
            &ctx(),
            Some(&json!({"value": "-1234.5"})),
        );
        assert_eq!(it.event_value.as_deref(), Some("-1.234,50"));
        let pct = run_script("AFPercent_Format(1, 0)", &ctx(), Some(&json!({"value": "0.256"})));
        assert_eq!(pct.event_value.as_deref(), Some("25.6%"));
        let range = run_script(
            "AFRange_Validate(true, 0, true, 100)",
            &ctx(),
            Some(&json!({"value": "150"})),
        );
        assert!(!range.event_rc && range.out[0].0 == "alert");
        let ok = run_script(
            "AFRange_Validate(true, 0, true, 100)",
            &ctx(),
            Some(&json!({"value": "50"})),
        );
        assert!(ok.event_rc);
        let phone = run_script(
            "AFSpecial_Format(2)",
            &ctx(),
            Some(&json!({"value": "5551234567"})),
        );
        assert_eq!(phone.event_value.as_deref(), Some("(555) 123-4567"));
        let custom = run_script(
            "event.value = (Number(this.getField('price').value) * 2).toFixed(1)",
            &ctx(),
            Some(&json!({"value": ""})),
        );
        assert_eq!(custom.event_value.as_deref(), Some("25.0"));
    }
}
