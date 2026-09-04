//! AcroForm corpus gate: model-parse + writeback exercise over a corpus doc.
//!
//! Usage: `corpus_gate <input.pdf> [output.pdf]`
//!
//! Emits one JSON line on stdout describing the document's form feature
//! surface and the result of exercising the writeback chain on the first
//! text field, first checkbox and first radio group. Exit code 0 unless the
//! document cannot be opened at all (parse-level failure).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent_forms::{
    apply_field_value, build_form_model, parse_acroform, FormFieldKind, WriteValue,
};
use std::fmt::Write as _;

fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' | '\t' => o.push(' '),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: corpus_gate <input.pdf> [out.pdf]");
    let output = args.next();

    let bytes = std::fs::read(&input).expect("read input");
    let doc_name = std::path::Path::new(&input)
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.clone());

    // --- Model side (pdf-syntax parse) ---
    let pdf = match pdf_syntax::Pdf::new(std::sync::Arc::new(bytes.clone())) {
        Ok(p) => p,
        Err(e) => {
            println!(
                "{{\"doc\":{},\"open_fail\":{}}}",
                js(&doc_name),
                js(&format!("{e:?}"))
            );
            std::process::exit(1);
        }
    };
    let model = parse_acroform(&pdf)
        .map(|t| build_form_model(&t))
        .unwrap_or_default();

    let mut n_text = 0;
    let mut n_multiline = 0;
    let mut n_comb = 0;
    let mut n_password = 0;
    let mut n_checkbox = 0;
    let mut n_radio = 0;
    let mut n_combo = 0;
    let mut n_listbox = 0;
    let mut n_push = 0;
    let mut n_sig = 0;
    let mut n_maxlen = 0;
    let mut n_readonly = 0;
    let mut n_required = 0;
    let mut n_nonascii_onstate = 0;
    let mut n_widgets = 0;
    let mut first_text: Option<String> = None;
    let mut first_checkbox: Option<String> = None;
    let mut first_radio: Option<(String, String)> = None;

    for f in &model {
        n_widgets += f.widgets.len();
        if f.max_len.is_some() {
            n_maxlen += 1;
        }
        if f.read_only {
            n_readonly += 1;
        }
        if f.required {
            n_required += 1;
        }
        match &f.kind {
            FormFieldKind::Text {
                multiline,
                comb,
                password,
            } => {
                n_text += 1;
                if *multiline {
                    n_multiline += 1;
                }
                if *comb {
                    n_comb += 1;
                }
                if *password {
                    n_password += 1;
                }
                if first_text.is_none() && !f.read_only {
                    first_text = Some(f.name.clone());
                }
            }
            FormFieldKind::Checkbox { on_state, .. } => {
                n_checkbox += 1;
                if !on_state.is_ascii() {
                    n_nonascii_onstate += 1;
                }
                if first_checkbox.is_none() && !f.read_only {
                    first_checkbox = Some(f.name.clone());
                }
            }
            FormFieldKind::RadioGroup { options } => {
                n_radio += 1;
                if options.iter().any(|o| !o.is_ascii()) {
                    n_nonascii_onstate += 1;
                }
                if first_radio.is_none() && !f.read_only {
                    if let Some(opt) = options.iter().find(|o| !o.is_empty()) {
                        first_radio = Some((f.name.clone(), opt.clone()));
                    }
                }
            }
            FormFieldKind::ComboBox { .. } => n_combo += 1,
            FormFieldKind::ListBox { .. } => n_listbox += 1,
            FormFieldKind::PushButton => n_push += 1,
            FormFieldKind::Signature => n_sig += 1,
        }
    }

    // --- Writeback side (lopdf) ---
    let mut fill_results = Vec::new();
    let mut saved_ok = false;
    if let Ok(mut ldoc) = lopdf::Document::load_mem(&bytes) {
        if let Some(name) = &first_text {
            let r = apply_field_value(&mut ldoc, name, WriteValue::Text("Gate Café € test"));
            fill_results.push((
                "text",
                name.clone(),
                r.map(|_| ()).map_err(|e| e.to_string()),
            ));
        }
        if let Some(name) = &first_checkbox {
            let r = apply_field_value(&mut ldoc, name, WriteValue::Checkbox(true));
            fill_results.push((
                "checkbox",
                name.clone(),
                r.map(|_| ()).map_err(|e| e.to_string()),
            ));
        }
        if let Some((name, opt)) = &first_radio {
            let r = apply_field_value(&mut ldoc, name, WriteValue::Radio(opt));
            fill_results.push((
                "radio",
                name.clone(),
                r.map(|_| ()).map_err(|e| e.to_string()),
            ));
        }
        if let Some(out) = &output {
            let mut buf = Vec::new();
            if ldoc.save_to(&mut buf).is_ok() {
                saved_ok = std::fs::write(out, &buf).is_ok();
            }
        } else {
            let mut buf = Vec::new();
            saved_ok = ldoc.save_to(&mut buf).is_ok();
        }
    }

    let mut fills = String::new();
    for (i, (kind, name, result)) in fill_results.iter().enumerate() {
        if i > 0 {
            fills.push(',');
        }
        match result {
            Ok(()) => {
                let _ = write!(
                    fills,
                    "{{\"kind\":\"{kind}\",\"field\":{},\"ok\":true}}",
                    js(name)
                );
            }
            Err(e) => {
                let _ = write!(
                    fills,
                    "{{\"kind\":\"{kind}\",\"field\":{},\"ok\":false,\"error\":{}}}",
                    js(name),
                    js(e)
                );
            }
        }
    }

    println!(
        "{{\"doc\":{},\"fields\":{},\"widgets\":{n_widgets},\"text\":{n_text},\"multiline\":{n_multiline},\"comb\":{n_comb},\"password\":{n_password},\"checkbox\":{n_checkbox},\"radio\":{n_radio},\"combo\":{n_combo},\"listbox\":{n_listbox},\"pushbutton\":{n_push},\"signature\":{n_sig},\"maxlen\":{n_maxlen},\"readonly\":{n_readonly},\"required\":{n_required},\"nonascii_onstates\":{n_nonascii_onstate},\"fills\":[{fills}],\"saved\":{saved_ok}}}",
        js(&doc_name),
        model.len(),
    );
}
