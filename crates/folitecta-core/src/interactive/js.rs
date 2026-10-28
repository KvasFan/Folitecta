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
    "util.printf(format, …) with %s %d %f %.nf, util.printd(format, date), util.scand(format, text)",
    "AFSimple_Calculate(\"SUM\" | \"PRD\" | \"AVG\" | \"MIN\" | \"MAX\", fields)",
    "AFNumber_Format(decimals, separators, negative, currency, symbol, before), AFNumber_Keystroke(…)",
    "AFPercent_Format(decimals, separators), AFPercent_Keystroke(…)",
    "AFSpecial_Format(0 zip | 1 zip+4 | 2 phone | 3 social security), AFSpecial_Keystroke(…)",
    "AFRange_Validate(above?, lowest, below?, highest)",
    "AFDate_FormatEx(format), AFDate_KeystrokeEx(format), AFDate_Format(n), AFDate_Keystroke(n)",
    "AFTime_FormatEx(format), AFTime_KeystrokeEx(format), AFTime_Format(n), AFTime_Keystroke(n)",
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
    printd: (fmt, d) => printDate(fmt, d instanceof Date ? d : new Date(d)),
    scand: (fmt, s) => scanDate(fmt, s),
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
  // A committed value that is no number at all (letters, no digit) is refused.
  const refuseUnless = (ok) => {
    if (ok) return;
    event.rc = false;
    app.alert('The value entered does not match the format of the field [ ' + ((event.target && event.target.name) || '') + ' ]');
  };
  const AFNumber_Keystroke = (dec, sepStyle) => {
    if (event.willCommit) {
      const v = String(event.value).trim();
      if (v) refuseUnless(/\d/.test(v) && !/[A-Za-zÀ-ÿ]/.test(v));
      return;
    }
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
    if (event.willCommit) {
      const v = String(event.value).trim();
      const digits = v.replace(/\D/g, '').length;
      const lengths = [[5], [5, 9], [7, 10], [9]][Number(psf)] || [];
      if (v) refuseUnless(!/[^0-9\-()\s.]/.test(v) && lengths.includes(digits));
      return;
    }
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

  // Dates and times as forms write them (`util.printd` formats: d dd ddd
  // dddd m mm mmm mmmm yy yyyy H HH h hh M MM s ss t tt; other characters
  // as they are). Month and day names in English; Italian names are read too.
  const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  const MESI = ['gennaio', 'febbraio', 'marzo', 'aprile', 'maggio', 'giugno', 'luglio', 'agosto', 'settembre', 'ottobre', 'novembre', 'dicembre'];
  const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const GIORNI = ['domenica', 'lunedì', 'martedì', 'mercoledì', 'giovedì', 'venerdì', 'sabato'];
  const dateTokens = (fmt) => String(fmt).match(/yyyy|yy|mmmm|mmm|mm|m|dddd|ddd|dd|d|HH|H|hh|h|MM|M|ss|s|tt|t|[^ymdHhMst]+|./g) || [];
  const printDate = (fmt, t) => dateTokens(fmt).map((k) => {
    const h12 = t.getHours() % 12 || 12;
    switch (k) {
      case 'yyyy': return pad(t.getFullYear(), 4);
      case 'yy': return pad(t.getFullYear() % 100, 2);
      case 'mmmm': return MONTHS[t.getMonth()];
      case 'mmm': return MONTHS[t.getMonth()].slice(0, 3);
      case 'mm': return pad(t.getMonth() + 1, 2);
      case 'm': return String(t.getMonth() + 1);
      case 'dddd': return DAYS[t.getDay()];
      case 'ddd': return DAYS[t.getDay()].slice(0, 3);
      case 'dd': return pad(t.getDate(), 2);
      case 'd': return String(t.getDate());
      case 'HH': return pad(t.getHours(), 2);
      case 'H': return String(t.getHours());
      case 'hh': return pad(h12, 2);
      case 'h': return String(h12);
      case 'MM': return pad(t.getMinutes(), 2);
      case 'M': return String(t.getMinutes());
      case 'ss': return pad(t.getSeconds(), 2);
      case 's': return String(t.getSeconds());
      case 'tt': return t.getHours() < 12 ? 'am' : 'pm';
      case 't': return t.getHours() < 12 ? 'a' : 'p';
      default: return k;
    }
  }).join('');
  // Read `s` written in `fmt`, leniently: the numbers and month names of
  // `s` in the order the format has them; null when that is no real date
  // or time (31 April, 25 o'clock).
  const scanDate = (fmt, s) => {
    // Names of week days say nothing more than the date: they are left out.
    const starts = (list, w) => w.length >= 3 && list.some((x) => x.toLowerCase().startsWith(w.toLowerCase()));
    const weekday = (w) => starts([...DAYS, ...GIORNI], w) && !starts([...MONTHS, ...MESI], w);
    const parts = (String(s == null ? '' : s).match(/\d+|[A-Za-zÀ-ÿ]+/g) || []).filter((p) => /^\d/.test(p) || !weekday(p));
    if (!parts.length) return null;
    const wanted = dateTokens(fmt).filter((k) => /^(yyyy|yy|mmmm|mmm|mm|m|dd|d|HH|H|hh|h|MM|M|ss|s|tt|t)$/.test(k));
    const now = new Date();
    let y = now.getFullYear(), mo = 0, d = 1, H = 0, Mi = 0, S = 0, pm = null, i = 0;
    for (const k of wanted) {
      if (k === 'tt' || k === 't') {
        const w = parts[i] || '';
        if (/^[ap]/i.test(w)) { pm = /^p/i.test(w); i++; }
        continue;
      }
      const p = parts[i++];
      if (p === undefined) return null;
      // A month by its name, or in figures where the format writes its name.
      if (/^(mmmm|mmm|mm|m)$/.test(k) && /^\D/.test(p)) {
        const w = p.toLowerCase();
        let n = MONTHS.findIndex((x) => w.length >= 3 && x.toLowerCase().startsWith(w));
        if (n < 0) n = MESI.findIndex((x) => w.length >= 3 && x.startsWith(w));
        if (n < 0) return null;
        mo = n;
        continue;
      }
      if (!/^\d+$/.test(p)) return null;
      const n = parseInt(p, 10);
      switch (k) {
        case 'yyyy': case 'yy': y = p.length <= 2 ? (n < 50 ? 2000 + n : 1900 + n) : n; break;
        case 'mmmm': case 'mmm': case 'mm': case 'm': mo = n - 1; break;
        case 'dd': case 'd': d = n; break;
        case 'HH': case 'H': case 'hh': case 'h': H = n; break;
        case 'MM': case 'M': Mi = n; break;
        case 'ss': case 's': S = n; break;
      }
    }
    // A word left over after the time: "3:15 pm".
    if (pm === null && /^[ap]m?$/i.test(parts[i] || '')) pm = /^p/i.test(parts[i]);
    if (pm !== null) {
      if (H < 1 || H > 12) return null;
      H = (H % 12) + (pm ? 12 : 0);
    }
    if (mo < 0 || mo > 11 || d < 1 || H > 23 || Mi > 59 || S > 59 || y < 1 || y > 9999) return null;
    const t = new Date(y, mo, d, H, Mi, S);
    if (t.getMonth() !== mo || t.getDate() !== d) return null;
    return t;
  };
  const DATE_FORMATS = ['m/d', 'm/d/yy', 'mm/dd/yy', 'mm/yy', 'd-mmm', 'd-mmm-yy', 'dd-mmm-yy', 'yy-mm-dd', 'mmm-yy', 'mmmm-yy', 'mmm d, yyyy', 'mmmm d, yyyy', 'm/d/yy h:MM tt', 'm/d/yy HH:MM'];
  const TIME_FORMATS = ['HH:MM', 'h:MM tt', 'HH:MM:ss', 'h:MM:ss tt'];
  const targetName = () => (event.target && event.target.name) || '';
  const AFDate_FormatEx = (fmt) => {
    if (String(event.value).trim() === '') return;
    const t = scanDate(fmt, event.value);
    if (t) event.value = printDate(fmt, t);
  };
  const AFDate_KeystrokeEx = (fmt) => {
    if (!event.willCommit) {
      const ch = String(event.change || '');
      if (ch && /[^0-9A-Za-zÀ-ÿ\s\/\-.:,]/.test(ch)) event.rc = false;
      return;
    }
    if (String(event.value).trim() === '') return;
    if (!scanDate(fmt, event.value)) {
      event.rc = false;
      app.alert('Invalid date/time: please ensure that the date/time exists. Field [ ' + targetName() + ' ] should match format ' + fmt);
    }
  };
  const AFDate_Format = (n) => AFDate_FormatEx(DATE_FORMATS[n] || DATE_FORMATS[0]);
  const AFDate_Keystroke = (n) => AFDate_KeystrokeEx(DATE_FORMATS[n] || DATE_FORMATS[0]);
  const AFTime_FormatEx = AFDate_FormatEx;
  const AFTime_KeystrokeEx = AFDate_KeystrokeEx;
  const AFTime_Format = (n) => AFDate_FormatEx(TIME_FORMATS[n] || TIME_FORMATS[0]);
  const AFTime_Keystroke = (n) => AFDate_KeystrokeEx(TIME_FORMATS[n] || TIME_FORMATS[0]);

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

    /// The result of `code` run alone.
    fn eval(code: &str) -> String {
        let r = run_script(code, &ctx(), None);
        assert!(r.ok, "{code}: {r:?}");
        r.result.unwrap_or_default()
    }

    #[test]
    fn dates_are_written_with_every_format_letter() {
        let d = "new Date(2026, 2, 5, 15, 7, 9)";
        for (fmt, want) in [
            ("dd/mm/yyyy", "05/03/2026"),
            ("d/m/yy", "5/3/26"),
            ("d mmmm yyyy", "5 March 2026"),
            ("ddd, mmm d", "Thu, Mar 5"),
            ("dddd", "Thursday"),
            ("HH:MM:ss", "15:07:09"),
            ("h:MM tt", "3:07 pm"),
            ("yyyy-mm-dd H.M.s t", "2026-03-05 15.7.9 p"),
        ] {
            assert_eq!(eval(&format!("util.printd('{fmt}', {d})")), want, "{fmt}");
        }
    }

    #[test]
    fn dates_are_read_as_they_are_typed_and_only_when_real() {
        let read = |fmt: &str, text: &str| {
            eval(&format!(
                "const t = util.scand('{fmt}', '{text}'); t ? util.printd('yyyy-mm-dd HH:MM', t) : 'none'"
            ))
        };
        assert_eq!(read("dd/mm/yyyy", "5/3/2026"), "2026-03-05 00:00");
        assert_eq!(read("dd/mm/yyyy", "05-03-26"), "2026-03-05 00:00");
        assert_eq!(read("dd/mm/yyyy", "29/02/2024"), "2024-02-29 00:00");
        assert_eq!(read("dd/mm/yyyy", "29/02/2026"), "none");
        assert_eq!(read("dd/mm/yyyy", "31/04/2026"), "none");
        assert_eq!(read("dd/mm/yyyy", "5/13/2026"), "none");
        assert_eq!(read("dd/mm/yyyy", "tomorrow"), "none");
        assert_eq!(read("mm/dd/yyyy", "3/5/2026"), "2026-03-05 00:00");
        assert_eq!(read("d mmmm yyyy", "5 marzo 2026"), "2026-03-05 00:00");
        assert_eq!(read("d mmmm yyyy", "giovedì 5 March 2026"), "2026-03-05 00:00");
        assert_eq!(read("d mmmm yyyy", "5 3 2026"), "2026-03-05 00:00");
        assert_eq!(read("mmm d, yyyy", "Mar 5, 2026"), "2026-03-05 00:00");
        assert_eq!(read("d mmm yyyy", "5 mar 2026"), "2026-03-05 00:00");
        assert_eq!(
            read("h:MM tt", "3:15 pm"),
            format!("{}-01-01 15:15", eval("new Date().getFullYear()"))
        );
        assert_eq!(read("HH:MM", "25:00"), "none");
        assert_eq!(read("h:MM tt", "13:00 pm"), "none");
    }

    #[test]
    fn date_fields_format_and_refuse_dates_that_do_not_exist() {
        let field = |code: &str, value: &str| {
            run_script(
                code,
                &ctx(),
                Some(&json!({"value": value, "target": "price", "willCommit": true})),
            )
        };
        let r = field("AFDate_FormatEx('dd/mm/yyyy')", "5/3/26");
        assert_eq!(r.event_value.as_deref(), Some("05/03/2026"));
        let r = field("AFDate_Format(1)", "2026-3-5");
        assert_eq!(
            r.event_value.as_deref(),
            Some("2026-3-5"),
            "not a m/d/yy date: left as typed"
        );
        let r = field("AFDate_KeystrokeEx('dd/mm/yyyy')", "31/04/2026");
        assert!(!r.event_rc);
        assert!(
            r.out[0].1.contains("price") && r.out[0].1.contains("dd/mm/yyyy"),
            "{:?}",
            r.out
        );
        assert!(field("AFDate_KeystrokeEx('dd/mm/yyyy')", "30/04/2026").event_rc);
        assert!(field("AFDate_KeystrokeEx('dd/mm/yyyy')", "").event_rc);
        assert!(!field("AFTime_Keystroke(0)", "24:10").event_rc);
        assert_eq!(
            field("AFTime_Format(1)", "15:30").event_value.as_deref(),
            Some("3:30 pm")
        );
        // While typing, only the characters of a date pass.
        let typing = |change: &str| {
            run_script(
                "AFDate_KeystrokeEx('dd/mm/yyyy')",
                &ctx(),
                Some(&json!({"value": "", "change": change, "willCommit": false})),
            )
            .event_rc
        };
        assert!(typing("05/"));
        assert!(!typing("#"));
    }

    #[test]
    fn numbers_and_special_formats_are_checked_when_committed() {
        let commit = |code: &str, value: &str| {
            run_script(
                code,
                &ctx(),
                Some(&json!({"value": value, "target": "qty", "willCommit": true})),
            )
            .event_rc
        };
        assert!(commit("AFNumber_Keystroke(2, 2, 0, 0, '', true)", "1.234,50"));
        assert!(commit("AFNumber_Keystroke(2, 0, 0, 0, '€', true)", "€ 12"));
        assert!(!commit("AFNumber_Keystroke(2, 0, 0, 0, '', true)", "twelve"));
        assert!(!commit("AFPercent_Keystroke(1, 0)", "-"));
        assert!(commit("AFSpecial_Keystroke(0)", "20121"));
        assert!(!commit("AFSpecial_Keystroke(0)", "2012"));
        assert!(commit("AFSpecial_Keystroke(2)", "(555) 123-4567"));
        assert!(!commit("AFSpecial_Keystroke(3)", "12-34"));
    }
}
