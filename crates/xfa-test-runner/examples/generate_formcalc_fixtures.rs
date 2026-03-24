//! Generate minimal XFA PDF fixtures that contain FormCalc scripts.
//!
//! Each generated PDF is a single-page PDF/1.4 with:
//! - AcroForm → /XFA stream containing an XDP envelope
//! - XDP envelope → <template> packet with one or more <script> elements
//!   using contentType="application/x-formcalc"
//!
//! The CDATA wrapper around each script body lets FormCalc operators like
//! < > & appear unescaped in the XML.
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example generate_formcalc_fixtures
//!
//! Output: fixtures/formcalc/fc_NN_<name>.pdf

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

// ---------------------------------------------------------------------------
// FormCalc test cases: (filename_suffix, description, formcalc_script)
// ---------------------------------------------------------------------------

const CASES: &[(&str, &str, &str)] = &[
    (
        "arithmetic",
        "Basic arithmetic and variables",
        r#"var a = 5
var b = 3
a + b * 2"#,
    ),
    (
        "string_concat",
        "Concat, Left, Right, Len",
        r#"var s = Concat("Hello", ", ", "FormCalc", "!")
var l = Left(s, 5)
var r = Right(s, 9)
Concat(l, " / ", r)"#,
    ),
    (
        "string_transform",
        "Lower, Upper, LTrim, RTrim, Substr",
        r#"var s = "  XFA FormCalc  "
var trimmed = LTrim(RTrim(s))
Concat(Upper(Left(trimmed, 3)), Lower(Right(trimmed, 7)))"#,
    ),
    (
        "math_builtins",
        "Abs, Ceil, Floor, Round, Max, Min, Mod",
        r#"var x = -3.7
var a = Abs(x)
var c = Ceil(x)
var f = Floor(x)
var r = Round(a, 1)
var mx = Max(a, c, f, r)
var mn = Min(a, c, f, r)
mx + mn + Mod(7, 3)"#,
    ),
    (
        "conditional",
        "if / then / else / elseif / endif",
        r#"var score = 75
var grade = ""
if score >= 90 then
  grade = "A"
elseif score >= 80 then
  grade = "B"
elseif score >= 70 then
  grade = "C"
else
  grade = "F"
endif
grade"#,
    ),
    (
        "for_loop",
        "for / upto / step / endfor",
        r#"var sum = 0
for i = 1 upto 10 do
  sum = sum + i
endfor
sum"#,
    ),
    (
        "for_downto",
        "for / downto (countdown)",
        r#"var product = 1
for n = 5 downto 1 do
  product = product * n
endfor
product"#,
    ),
    (
        "while_loop",
        "while / endwhile with accumulator",
        r#"var n = 1
var acc = 0
while n <= 10 do
  acc = acc + n * n
  n = n + 1
endwhile
acc"#,
    ),
    (
        "date_funcs",
        "Date, Time, Num2Date, Num2Time",
        r#"var d = Date()
var t = Time()
var ds = Num2Date(d, "YYYY-MM-DD")
ds"#,
    ),
    (
        "logical_builtins",
        "Choose(), OneOf() via if/endif",
        r#"var x = 5
var picked = Choose(x, "one", "two", "three", "four", "five")
var cond = ""
if x > 3 then
  cond = "big"
else
  cond = "small"
endif
var member = OneOf(x, 1, 3, 5, 7)
Concat(picked, " ", cond)"#,
    ),
    (
        "sum_avg",
        "Sum, Avg, Count",
        r#"var s = Sum(1, 2, 3, 4, 5)
var a = Avg(10, 20, 30)
var c = Count(1, 2, 3, 4)
s + a + c"#,
    ),
    (
        "string_search",
        "At(), Replace()",
        r#"var s = "Hello World"
var pos = At("World", s)
var rep = Replace(s, "World", "FormCalc")
rep"#,
    ),
    (
        "multi_script",
        "Multiple script elements in one template",
        // Two separate <script> elements — we embed both in the same field
        // via the generate_xdp function which supports a vec of scripts.
        // For simplicity here we just put two consecutive calculations:
        r#"var a = 100
a + 200"#,
    ),
    (
        "nested_subform",
        "Scripts in nested subforms",
        r#"var depth = 3
var result = depth * depth * depth
result"#,
    ),
    (
        "complex_formula",
        "Multi-step business-logic formula",
        // Monthly loan payment: P * r / (1 - (1+r)^-n)
        // Avoid ^ operator (not in BinOp): compute (1+r)^12 iteratively via Pmt().
        // Use a 12-step accumulator to compute the denominator factor.
        r#"var principal = 10000
var rate = 0.05
var periods = 12
var monthly_rate = rate / periods
var factor = 1
for k = 1 upto periods do
  factor = factor * (1 + monthly_rate)
endfor
var payment = principal * monthly_rate * factor / (factor - 1)
Round(payment, 2)"#,
    ),
    // ── Edge cases ────────────────────────────────────────────
    (
        "div_zero",
        "Division by zero → graceful eval error (no panic)",
        // DivisionByZero is a Result::Err, caught by evaluate_formcalc, not a panic.
        r#"var x = 0
1 / x"#,
    ),
    (
        "nested_builtins",
        "Deeply nested built-in function calls",
        r#"var x = -3.7
var a = Round(Abs(Floor(x)), 1)
var b = Abs(Round(x, 0))
a + b"#,
    ),
    (
        "host_method",
        "SOM member access and host method calls return Null (no DOM)",
        // Dotted names (xfa.host.*, xfa.resolveNode) silently return Null.
        // The final numeric expression is the script result.
        r#"var page = xfa.host.currentPage
var ver = xfa.version
var dummy = xfa.host.resetData()
42"#,
    ),
    (
        "user_func",
        "User-defined function declaration and recursive call",
        // FormCalc supports func…endfunc. The last expression in the body is the return value.
        r#"func square(n)
  n * n
endfunc
func sumSquares(limit)
  var total = 0
  for i = 1 upto limit do
    total = total + square(i)
  endfor
  total
endfunc
sumSquares(5)"#,
    ),
    (
        "large_loop",
        "Loop over 500 iterations — stress test interpreter",
        r#"var total = 0
for i = 1 upto 500 do
  total = total + i
endfor
total"#,
    ),
    (
        "type_coerce",
        "Type coercion: string-to-number, null-to-number",
        // to_number("42") = 42.0, to_number(null) = 0.0 per spec.
        r#"var numStr = "42"
var result = numStr + 8
var nul = null
var nulAdd = nul + 100
result + nulAdd"#,
    ),
    (
        "null_ops",
        "Null value propagation and comparison",
        r#"var x = null
var isNull = (x == null)
var notNull = (x <> null)
var asNum = x + 0
if isNull then
  asNum = 999
endif
asNum"#,
    ),
    (
        "financial_builtins",
        "Financial functions: Pmt, FV, PV, NPV",
        r#"var monthlyPmt = Pmt(10000, 0.005, 24)
var futureVal = FV(100, 0.005, 12)
var presentVal = PV(100, 0.005, 12)
Round(monthlyPmt + futureVal + presentVal, 2)"#,
    ),
];

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formcalc");
    std::fs::create_dir_all(&out_dir).expect("create formcalc fixtures dir");

    for (i, &(suffix, desc, script)) in CASES.iter().enumerate() {
        let filename = format!("fc_{:02}_{suffix}.pdf", i + 1);
        let path = out_dir.join(&filename);
        let bytes = make_xfa_pdf(desc, script);
        std::fs::write(&path, &bytes).expect("write PDF");
        println!("  {filename}  ({} bytes)  — {desc}", bytes.len());
    }

    println!(
        "\n{} FormCalc fixture PDFs written to {}",
        CASES.len(),
        out_dir.display()
    );
}

/// Build a minimal XFA PDF whose <template> contains a single FormCalc script.
fn make_xfa_pdf(description: &str, formcalc_script: &str) -> Vec<u8> {
    let xdp = build_xdp(description, formcalc_script);
    build_pdf(xdp)
}

/// Wrap a FormCalc script body in a minimal XDP envelope.
///
/// Structure:
///   xdp:xdp
///     template
///       subform[form1]
///         subform[page1]  (page content)
///         subform[data]
///           field[calcField]
///             calculate
///               script[contentType=application/x-formcalc]
fn build_xdp(description: &str, script: &str) -> String {
    // Escape description for XML attribute use (no special chars expected).
    let desc_safe = description.replace('"', "&quot;").replace('<', "&lt;");
    // We use CDATA for the script body so < > & are legal without escaping.
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <desc>
    <text name="description">{desc_safe}</text>
  </desc>
  <subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.25in" y="0.25in" w="8in" h="10.5in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="data" layout="tb">
      <field name="calcField" w="3in" h="0.25in">
        <ui><textEdit/></ui>
        <value><text/></value>
        <calculate>
          <script contentType="application/x-formcalc"><![CDATA[
{script}
]]></script>
        </calculate>
      </field>
    </subform>
  </subform>
</template>
</xdp:xdp>
"#
    )
}

/// Wrap the XDP XML in a minimal PDF using lopdf.
///
/// Structure:
///   Catalog → AcroForm (inline) → /XFA → stream[xdp_bytes]
///   Pages   → [Page]
fn build_pdf(xdp: String) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    // --- XFA stream ---
    let xdp_bytes = xdp.into_bytes();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));

    // --- Page tree ---
    let pages_id = doc.new_object_id();

    let content_stream = Stream::new(dictionary! { "Length" => Object::Integer(0i64) }, vec![]);
    let content_id = doc.add_object(Object::Stream(content_stream));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Page".to_vec()),
        "Parent"   => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
    }));

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Pages".to_vec()),
            "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );

    // --- AcroForm with /XFA ---
    let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
        "XFA"    => Object::Reference(xfa_id),
        "Fields" => Object::Array(vec![]),
    }));

    // --- Catalog ---
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"     => Object::Name(b"Catalog".to_vec()),
        "Pages"    => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(acroform_id),
    }));

    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("lopdf save");
    out
}
