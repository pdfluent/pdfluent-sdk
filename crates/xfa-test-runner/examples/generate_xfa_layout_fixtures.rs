//! Generate minimal XFA PDF fixtures that exercise layout features.
//!
//! Each generated PDF is a single-file PDF/1.4 with an AcroForm → /XFA stream
//! containing a full XDP envelope.  The fixtures cover structural XFA features
//! that are independent of FormCalc scripting:
//!
//!   1. Simple single-page form with labeled text fields
//!   2. Multi-column row layout (side-by-side fields)
//!   3. Page overflow / multi-page form
//!   4. Conditional field visibility (presence=hidden vs visible)
//!   5. Master page with header/footer bands
//!   6. Table-style subform (rows and cells)
//!   7. Nested subforms (subform-in-subform)
//!   8. Exclusion groups (radio-button-style)
//!   9. Numeric / date / check-box field types
//!  10. Mixed layout: paginate + tb inside one subform
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example generate_xfa_layout_fixtures
//!
//! Output: fixtures/xfa-layout/xl_NN_<name>.pdf

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xfa-layout");
    std::fs::create_dir_all(&out_dir).expect("create xfa-layout fixtures dir");

    let cases: Vec<(&str, &str, String)> = vec![
        (
            "simple_form",
            "Single-page form with labeled text fields",
            build_simple_form(),
        ),
        (
            "row_layout",
            "Multi-column row layout (side-by-side fields)",
            build_row_layout(),
        ),
        (
            "multipage",
            "Multi-page form (overflow to page 2)",
            build_multipage(),
        ),
        (
            "conditional_visibility",
            "Conditional field visibility (presence=hidden/visible)",
            build_conditional_visibility(),
        ),
        (
            "master_page",
            "Master page with header and footer bands",
            build_master_page(),
        ),
        (
            "table_layout",
            "Table-style subform with rows and cells",
            build_table_layout(),
        ),
        (
            "nested_subforms",
            "Deeply nested subform hierarchy",
            build_nested_subforms(),
        ),
        (
            "exclusion_group",
            "Exclusion group (radio button alternatives)",
            build_exclusion_group(),
        ),
        (
            "field_types",
            "Numeric, date, checkbox, and signature field types",
            build_field_types(),
        ),
        (
            "mixed_layout",
            "Mixed paginate+tb layout with multiple content areas",
            build_mixed_layout(),
        ),
        // ── Edge cases ────────────────────────────────────────
        (
            "rtl_layout",
            "RTL locale and right-to-left field order",
            build_rtl_layout(),
        ),
        (
            "keep_together",
            "Keep-together: subform that must not be split across pages",
            build_keep_together(),
        ),
        (
            "dynamic_table",
            "Dynamic table with occur min/max for growing rows",
            build_dynamic_table(),
        ),
        (
            "nested_page_breaks",
            "Nested subforms each triggering a page break",
            build_nested_page_breaks(),
        ),
        (
            "relevance_expr",
            "Conditional relevance expressions on fields and subforms",
            build_relevance_expr(),
        ),
    ];

    for (i, (suffix, desc, xdp)) in cases.iter().enumerate() {
        let filename = format!("xl_{:02}_{suffix}.pdf", i + 1);
        let path = out_dir.join(&filename);
        let bytes = build_pdf(xdp.clone());
        std::fs::write(&path, &bytes).expect("write PDF");
        println!("  {filename}  ({} bytes)  — {desc}", bytes.len());
    }

    println!(
        "\n{} XFA layout fixture PDFs written to {}",
        cases.len(),
        out_dir.display()
    );
}

// ---------------------------------------------------------------------------
// XDP builders
// ---------------------------------------------------------------------------

fn xdp_wrap(desc: &str, template_body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <desc>
    <text name="description">{desc}</text>
  </desc>
  {template_body}
</template>
</xdp:xdp>
"#,
        desc = desc.replace('"', "&quot;").replace('<', "&lt;"),
        template_body = template_body,
    )
}

/// Standard page area used by most fixtures (US Letter, 0.5in margins).
const PAGE_AREA: &str = r#"<pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>"#;

fn text_field(name: &str, caption: &str, w: &str, h: &str) -> String {
    format!(
        r#"<field name="{name}" w="{w}" h="{h}">
          <caption><value><text>{caption}</text></value></caption>
          <ui><textEdit/></ui>
          <value><text/></value>
        </field>"#
    )
}

fn build_simple_form() -> String {
    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    {PAGE_AREA}
    <subform name="section1" layout="tb" w="7.5in">
      {first_name}
      {last_name}
      {email}
      {phone}
    </subform>
  </subform>"#,
        first_name = text_field("firstName", "First Name", "3.5in", "0.3in"),
        last_name = text_field("lastName", "Last Name", "3.5in", "0.3in"),
        email = text_field("email", "Email Address", "5in", "0.3in"),
        phone = text_field("phone", "Phone Number", "3in", "0.3in"),
    );
    xdp_wrap("Single-page form with labeled text fields", &body)
}

fn build_row_layout() -> String {
    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    {PAGE_AREA}
    <subform name="row1" layout="lr-tb" w="7.5in" h="0.4in">
      {first}
      {last}
    </subform>
    <subform name="row2" layout="lr-tb" w="7.5in" h="0.4in">
      {city}
      {state}
      {zip}
    </subform>
  </subform>"#,
        first = text_field("firstName", "First", "3.5in", "0.4in"),
        last = text_field("lastName", "Last", "3.5in", "0.4in"),
        city = text_field("city", "City", "3in", "0.4in"),
        state = text_field("state", "State", "1.5in", "0.4in"),
        zip = text_field("zip", "ZIP", "1.5in", "0.4in"),
    );
    xdp_wrap("Multi-column row layout", &body)
}

fn build_multipage() -> String {
    // Generate enough fields to fill multiple pages.
    let mut fields = String::new();
    for i in 1..=25 {
        fields.push_str(&text_field(
            &format!("field{i}"),
            &format!("Field {i}"),
            "7in",
            "0.45in",
        ));
        fields.push('\n');
    }

    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
      <pageArea name="PageN" id="PageN">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="data" layout="tb" w="7.5in">
      {fields}
    </subform>
  </subform>"#
    );
    xdp_wrap("Multi-page form (overflow to page 2)", &body)
}

fn build_conditional_visibility() -> String {
    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    {PAGE_AREA}
    <subform name="section" layout="tb" w="7.5in">
      <!-- visible field -->
      <field name="visibleField" w="4in" h="0.3in" presence="visible">
        <caption><value><text>Visible Field</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>I am visible</text></value>
      </field>
      <!-- hidden field (invisible but participates in layout) -->
      <field name="hiddenField" w="4in" h="0.3in" presence="hidden">
        <caption><value><text>Hidden Field</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>I am hidden</text></value>
      </field>
      <!-- invisible field (does not participate in layout) -->
      <field name="invisibleField" w="4in" h="0.3in" presence="invisible">
        <caption><value><text>Invisible Field</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>I am invisible</text></value>
      </field>
      <!-- protected field (visible, read-only) -->
      <field name="protectedField" w="4in" h="0.3in" presence="visible" access="readOnly">
        <caption><value><text>Read-Only Field</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>Read-only content</text></value>
      </field>
    </subform>
  </subform>"#
    );
    xdp_wrap("Conditional field visibility", &body)
}

fn build_master_page() -> String {
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <!-- Header band at top of page -->
        <contentArea x="0.5in" y="1in" w="7.5in" h="9.5in"/>
        <medium stock="default" short="8.5in" long="11in"/>
        <!-- Header field positioned in the top 0.5in margin -->
        <draw name="pageHeader" x="0.5in" y="0.25in" w="7.5in" h="0.4in">
          <value><text>Company Name — Page Header</text></value>
        </draw>
        <!-- Footer field positioned at the bottom of the page -->
        <draw name="pageFooter" x="0.5in" y="10.5in" w="7.5in" h="0.3in">
          <value><text>Confidential — Page 1</text></value>
        </draw>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <field name="content" w="7in" h="0.3in">
        <caption><value><text>Body Content</text></value></caption>
        <ui><textEdit/></ui>
        <value><text/></value>
      </field>
    </subform>
  </subform>"#;
    xdp_wrap("Master page with header and footer bands", body)
}

fn build_table_layout() -> String {
    // XFA table: subform layout="table" with subform layout="row" children.
    let row = |label: &str, val1: &str, val2: &str| {
        format!(
            r#"<subform layout="row" w="7.5in" h="0.3in">
          <field name="col0" w="2.5in" h="0.3in">
            <ui><textEdit/></ui><value><text>{label}</text></value>
          </field>
          <field name="col1" w="2.5in" h="0.3in">
            <ui><textEdit/></ui><value><text>{val1}</text></value>
          </field>
          <field name="col2" w="2.5in" h="0.3in">
            <ui><textEdit/></ui><value><text>{val2}</text></value>
          </field>
        </subform>"#
        )
    };

    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    {PAGE_AREA}
    <subform name="table" layout="table" w="7.5in">
      {header}
      {r1}
      {r2}
      {r3}
    </subform>
  </subform>"#,
        header = row("Product", "Quantity", "Price"),
        r1 = row("Widget A", "10", "$5.00"),
        r2 = row("Widget B", "5", "$12.00"),
        r3 = row("Widget C", "20", "$3.50"),
    );
    xdp_wrap("Table-style subform", &body)
}

fn build_nested_subforms() -> String {
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="level1" layout="tb" w="7.5in">
      <subform name="level2a" layout="tb" w="7in">
        <subform name="level3a" layout="tb" w="6.5in">
          <field name="deepField" w="4in" h="0.3in">
            <caption><value><text>Deeply Nested Field</text></value></caption>
            <ui><textEdit/></ui>
            <value><text>depth 3</text></value>
          </field>
        </subform>
      </subform>
      <subform name="level2b" layout="lr-tb" w="7in">
        <field name="sideA" w="3in" h="0.3in">
          <caption><value><text>Side A</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
        <field name="sideB" w="3in" h="0.3in">
          <caption><value><text>Side B</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
      </subform>
    </subform>
  </subform>"#;
    xdp_wrap("Deeply nested subform hierarchy", body)
}

fn build_exclusion_group() -> String {
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb" w="7.5in">
      <draw name="label" w="5in" h="0.3in">
        <value><text>Please select your preferred contact method:</text></value>
      </draw>
      <exclGroup name="contactMethod" layout="tb" w="5in">
        <field name="byEmail" h="0.3in">
          <caption><value><text>Email</text></value></caption>
          <ui><checkButton shape="round" size="0.18in"/></ui>
          <value><integer>0</integer></value>
        </field>
        <field name="byPhone" h="0.3in">
          <caption><value><text>Phone</text></value></caption>
          <ui><checkButton shape="round" size="0.18in"/></ui>
          <value><integer>0</integer></value>
        </field>
        <field name="byPost" h="0.3in">
          <caption><value><text>Post</text></value></caption>
          <ui><checkButton shape="round" size="0.18in"/></ui>
          <value><integer>0</integer></value>
        </field>
      </exclGroup>
    </subform>
  </subform>"#;
    xdp_wrap("Exclusion group (radio button alternatives)", body)
}

fn build_field_types() -> String {
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="fields" layout="tb" w="7.5in">
      <!-- Numeric field -->
      <field name="amount" w="3in" h="0.3in">
        <caption><value><text>Amount (EUR)</text></value></caption>
        <ui><numericEdit/></ui>
        <value><float>0</float></value>
        <format>
          <picture>num{z,zzz,zzz.zz}</picture>
        </format>
      </field>
      <!-- Date field -->
      <field name="birthDate" w="3in" h="0.3in">
        <caption><value><text>Date of Birth</text></value></caption>
        <ui><dateTimeEdit/></ui>
        <value><date/></value>
        <format>
          <picture>date{DD/MM/YYYY}</picture>
        </format>
      </field>
      <!-- Checkbox field -->
      <field name="agree" w="4in" h="0.3in">
        <caption><value><text>I agree to the terms and conditions</text></value></caption>
        <ui><checkButton size="0.18in"/></ui>
        <value><integer>0</integer></value>
        <items>
          <integer>1</integer>
          <integer>0</integer>
        </items>
      </field>
      <!-- Drop-down list -->
      <field name="country" w="3in" h="0.3in">
        <caption><value><text>Country</text></value></caption>
        <ui><choiceList open="userControl"/></ui>
        <value><text/></value>
        <items>
          <text>Netherlands</text>
          <text>Germany</text>
          <text>Belgium</text>
          <text>France</text>
        </items>
      </field>
      <!-- Signature field -->
      <field name="signature" w="5in" h="1in">
        <caption><value><text>Signature</text></value></caption>
        <ui><signature/></ui>
      </field>
      <!-- Password field -->
      <field name="pin" w="2in" h="0.3in">
        <caption><value><text>PIN</text></value></caption>
        <ui><passwordEdit/></ui>
        <value><text/></value>
      </field>
    </subform>
  </subform>"#;
    xdp_wrap("Numeric, date, checkbox, and signature field types", body)
}

fn build_mixed_layout() -> String {
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <!-- Top section: top-to-bottom layout -->
    <subform name="header" layout="tb" w="7.5in">
      <draw name="title" w="7.5in" h="0.5in">
        <value><text>Multi-Layout XFA Form</text></value>
        <font size="18pt" typeface="Arial"/>
      </draw>
      <draw name="subtitle" w="7.5in" h="0.25in">
        <value><text>Demonstrates mixed layout modes</text></value>
      </draw>
    </subform>
    <!-- Middle section: left-right-top-bottom row layout -->
    <subform name="twoColumn" layout="lr-tb" w="7.5in">
      <subform name="leftColumn" layout="tb" w="3.5in">
        <field name="field1" w="3.5in" h="0.3in">
          <caption><value><text>Left field 1</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
        <field name="field2" w="3.5in" h="0.3in">
          <caption><value><text>Left field 2</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
      </subform>
      <subform name="rightColumn" layout="tb" w="3.5in">
        <field name="field3" w="3.5in" h="0.3in">
          <caption><value><text>Right field 1</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
        <field name="field4" w="3.5in" h="0.3in">
          <caption><value><text>Right field 2</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
      </subform>
    </subform>
    <!-- Bottom section: free-positioned fields (absolute x/y) -->
    <subform name="freePos" layout="position" w="7.5in" h="3in">
      <field name="label" x="0.5in" y="0.5in" w="3in" h="0.3in">
        <caption><value><text>Absolutely positioned</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="label2" x="4in" y="1.5in" w="3in" h="0.3in">
        <caption><value><text>Also positioned</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
    </subform>
  </subform>"#;
    xdp_wrap("Mixed paginate+tb layout with multiple content areas", body)
}

fn build_rtl_layout() -> String {
    // RTL locale + Arabic-style field labelling.
    // XFA layout direction is controlled by the locale; fields still flow lr-tb
    // in our minimal fixture but the locale string exercises locale parsing.
    let body = r#"<subform name="form1" layout="paginate" locale="ar_AE">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="rtlSection" layout="rl-tb" w="7.5in">
      <field name="familyName" w="3.5in" h="0.3in">
        <caption placement="right"><value><text>اسم العائلة</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="givenName" w="3.5in" h="0.3in">
        <caption placement="right"><value><text>الاسم الأول</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="idNumber" w="3in" h="0.3in">
        <caption placement="right"><value><text>رقم الهوية</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
    </subform>
  </subform>"#;
    xdp_wrap("RTL locale and right-to-left field order", body)
}

fn build_keep_together() -> String {
    // keep="contentArea" on a subform prevents it from being split across pages.
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="3in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
      <pageArea name="PageN" id="PageN">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <!-- This section fills most of the first page -->
    <subform name="filler" layout="tb" w="7.5in">
      <field name="f1" w="7in" h="0.8in"><ui><textEdit multiLine="1"/></ui><value><text/></value></field>
      <field name="f2" w="7in" h="0.8in"><ui><textEdit multiLine="1"/></ui><value><text/></value></field>
    </subform>
    <!-- keep="contentArea" forces this block to start on a new page if it won't fit -->
    <subform name="keepBlock" layout="tb" w="7.5in" keep="contentArea">
      <field name="header" w="7in" h="0.3in"><ui><textEdit/></ui><value><text>Keep-together block</text></value></field>
      <field name="line1" w="7in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="line2" w="7in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="line3" w="7in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;
    xdp_wrap(
        "Keep-together: subform that must not be split across pages",
        body,
    )
}

fn build_dynamic_table() -> String {
    // occur element allows a subform to repeat min/max times (dynamic table row).
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="invoiceTable" layout="tb" w="7.5in">
      <!-- Header row (static) -->
      <subform name="headerRow" layout="lr-tb" w="7.5in" h="0.35in">
        <field name="hProduct" w="3in" h="0.35in"><ui><textEdit/></ui><value><text>Product</text></value></field>
        <field name="hQty" w="1.5in" h="0.35in"><ui><textEdit/></ui><value><text>Qty</text></value></field>
        <field name="hPrice" w="1.5in" h="0.35in"><ui><textEdit/></ui><value><text>Unit Price</text></value></field>
        <field name="hTotal" w="1.5in" h="0.35in"><ui><textEdit/></ui><value><text>Total</text></value></field>
      </subform>
      <!-- Repeating data row: occur min=1 max=-1 means 1 to unlimited -->
      <subform name="dataRow" layout="lr-tb" w="7.5in" h="0.3in">
        <occur min="1" max="-1"/>
        <field name="product" w="3in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="qty" w="1.5in" h="0.3in"><ui><numericEdit/></ui><value><float>0</float></value></field>
        <field name="unitPrice" w="1.5in" h="0.3in"><ui><numericEdit/></ui><value><float>0</float></value></field>
        <field name="lineTotal" w="1.5in" h="0.3in"><ui><numericEdit/></ui><value><float>0</float></value></field>
      </subform>
      <!-- Footer row (static) -->
      <subform name="footerRow" layout="lr-tb" w="7.5in" h="0.35in">
        <field name="fLabel" w="6in" h="0.35in"><ui><textEdit/></ui><value><text>Grand Total:</text></value></field>
        <field name="fTotal" w="1.5in" h="0.35in"><ui><numericEdit/></ui><value><float>0</float></value></field>
      </subform>
    </subform>
  </subform>"#;
    xdp_wrap("Dynamic table with occur min/max for growing rows", body)
}

fn build_nested_page_breaks() -> String {
    // Multiple nested subforms, each with enough content to overflow.
    let many_fields: String = (1..=8)
        .map(|i| {
            format!(
                r#"<field name="f{i}" w="7in" h="0.5in">
          <ui><textEdit multiLine="1"/></ui><value><text>Field {i} content</text></value>
        </field>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let body = format!(
        r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="4in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
      <pageArea name="PageN" id="PageN">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section1" layout="tb" w="7.5in">
      <draw name="h1" w="7in" h="0.3in"><value><text>Section 1</text></value></draw>
      {many_fields}
    </subform>
    <subform name="section2" layout="tb" w="7.5in">
      <draw name="h2" w="7in" h="0.3in"><value><text>Section 2 (on overflow page)</text></value></draw>
      <field name="sec2f1" w="7in" h="0.3in"><ui><textEdit/></ui><value><text>Section 2 data</text></value></field>
    </subform>
  </subform>"#
    );
    xdp_wrap("Nested subforms each triggering a page break", &body)
}

fn build_relevance_expr() -> String {
    // relevant attribute controls conditional visibility via a SOM expression.
    // "+1" means visible, "-1" means hidden. Can also be a SOM path expression.
    let body = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="section" layout="tb" w="7.5in">
      <!-- Always visible -->
      <field name="accountType" w="4in" h="0.3in">
        <caption><value><text>Account Type</text></value></caption>
        <ui><choiceList open="userControl"/></ui>
        <value><text>personal</text></value>
        <items><text>personal</text><text>business</text></items>
      </field>
      <!-- Only visible for business accounts (relevant expression) -->
      <subform name="businessSection" layout="tb" w="7.5in" relevant="+business -personal">
        <field name="companyReg" w="4in" h="0.3in">
          <caption><value><text>Company Registration</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
        <field name="vatNumber" w="4in" h="0.3in">
          <caption><value><text>VAT Number</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
      </subform>
      <!-- Only visible for personal accounts -->
      <subform name="personalSection" layout="tb" w="7.5in" relevant="+personal -business">
        <field name="nationalId" w="4in" h="0.3in">
          <caption><value><text>National ID</text></value></caption>
          <ui><textEdit/></ui><value><text/></value>
        </field>
      </subform>
      <!-- Field with presence controlled by calculate script -->
      <field name="extraField" w="4in" h="0.3in" presence="hidden">
        <caption><value><text>Conditional Extra Field</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
    </subform>
  </subform>"#;
    xdp_wrap(
        "Conditional relevance expressions on fields and subforms",
        body,
    )
}

// ---------------------------------------------------------------------------
// PDF builder (same as formcalc generator)
// ---------------------------------------------------------------------------

fn build_pdf(xdp: String) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    let xdp_bytes = xdp.into_bytes();
    let xfa_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
        xdp_bytes,
    );
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));

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

    let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
        "XFA"    => Object::Reference(xfa_id),
        "Fields" => Object::Array(vec![]),
    }));

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
