//! Generate XFA PDF fixtures with both template AND datasets packets.
//!
//! These fixtures make the `xfa_data_roundtrip` test execute (instead of skip).
//! Each PDF embeds an XDP envelope with:
//!   <template>  — form structure with bound fields
//!   <xfa:datasets> — pre-filled data values
//!
//! Test coverage:
//!   xd_01  Personal info: name, email, phone
//!   xd_02  Address: street, city, postal code, country
//!   xd_03  Date fields: birth date, issue date, expiry date
//!   xd_04  Repeating rows: table with 3 invoice line items
//!   xd_05  Combined: personal + address + dates in one form
//!   xd_06  Nested groups: company → department → employee
//!   xd_07  Unicode values: accented chars, special symbols
//!   xd_08  Numeric values: integers and decimals
//!   xd_09  Multi-page: 10 fields spanning two pages
//!   xd_10  Empty + non-empty mix: some fields have data, some don't
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example generate_xfa_data_fixtures
//!
//! Output: fixtures/xfa-data/xd_NN_<name>.pdf

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xfa-data");
    std::fs::create_dir_all(&out_dir).expect("create xfa-data fixtures dir");

    // Static cases: (suffix, description, template_body, data_body)
    let mut cases: Vec<(&str, &str, String, String)> = vec![
        (
            "personal_info",
            "Personal info: name, email, phone",
            TMPL_PERSONAL.to_string(),
            DATA_PERSONAL.to_string(),
        ),
        (
            "address",
            "Address fields: street, city, postal code, country",
            TMPL_ADDRESS.to_string(),
            DATA_ADDRESS.to_string(),
        ),
        (
            "date_fields",
            "Date fields: birth date, issue date, expiry date",
            TMPL_DATES.to_string(),
            DATA_DATES.to_string(),
        ),
        (
            "repeating_rows",
            "Repeating rows: 3 invoice line items",
            TMPL_ROWS.to_string(),
            DATA_ROWS.to_string(),
        ),
        (
            "combined",
            "Combined: personal + address + dates",
            TMPL_COMBINED.to_string(),
            DATA_COMBINED.to_string(),
        ),
        (
            "nested_groups",
            "Nested groups: company → department → employee",
            TMPL_NESTED.to_string(),
            DATA_NESTED.to_string(),
        ),
        (
            "unicode_values",
            "Unicode values: accented chars and symbols",
            TMPL_UNICODE.to_string(),
            DATA_UNICODE.to_string(),
        ),
        (
            "numeric_values",
            "Numeric values: integers and decimals",
            TMPL_NUMERIC.to_string(),
            DATA_NUMERIC.to_string(),
        ),
        (
            "multipage_data",
            "Multi-page: 10 data-bound fields",
            TMPL_MULTIPAGE.to_string(),
            DATA_MULTIPAGE.to_string(),
        ),
        (
            "mixed_empty",
            "Mix: some fields filled, some empty",
            TMPL_MIXED.to_string(),
            DATA_MIXED.to_string(),
        ),
    ];

    // Edge cases with dynamically generated content
    cases.push((
        "null_values",
        "Null and empty element values in datasets",
        TMPL_NULL_VALUES.to_string(),
        DATA_NULL_VALUES.to_string(),
    ));
    cases.push((
        "deep_nesting",
        "Deeply nested data groups (5 levels)",
        TMPL_DEEP.to_string(),
        DATA_DEEP.to_string(),
    ));
    cases.push((
        "cjk_unicode",
        "CJK characters, emoji (via XML entity), and mixed scripts",
        TMPL_CJK.to_string(),
        DATA_CJK.to_string(),
    ));
    cases.push((
        "empty_collection",
        "Empty collection element (0 child items)",
        TMPL_EMPTY_COLL.to_string(),
        DATA_EMPTY_COLL.to_string(),
    ));

    // Large dataset: 100 fields generated at runtime
    let (large_tmpl, large_data) = build_large_dataset(100);
    cases.push((
        "large_dataset",
        "Large dataset: 100 data-bound fields",
        large_tmpl,
        large_data,
    ));

    // ── Edge cases ────────────────────────────────────────────────
    cases.push((
        "null_nested_values",
        "Null/missing values at multiple nesting levels",
        TMPL_NULL_NESTED.to_string(),
        DATA_NULL_NESTED.to_string(),
    ));
    cases.push((
        "empty_array_element",
        "Collection element present but containing no children",
        TMPL_EMPTY_ARR.to_string(),
        DATA_EMPTY_ARR.to_string(),
    ));
    cases.push((
        "six_levels_deep",
        "Data nesting 6 levels deep (beyond standard 5-level test)",
        TMPL_SIX_DEEP.to_string(),
        DATA_SIX_DEEP.to_string(),
    ));
    cases.push((
        "xml_special_chars",
        "Data values containing XML special characters (&, <, >, \", ')",
        TMPL_XML_CHARS.to_string(),
        DATA_XML_CHARS.to_string(),
    ));
    cases.push((
        "sibling_duplicates",
        "Multiple sibling elements with the same tag name (repeating rows)",
        TMPL_SIBLINGS.to_string(),
        DATA_SIBLINGS.to_string(),
    ));

    // ── Large data ────────────────────────────────────────────────
    let (fifty_tmpl, fifty_data) = build_large_dataset(50);
    cases.push((
        "fifty_fields",
        "Exactly 50 data-bound fields",
        fifty_tmpl,
        fifty_data,
    ));
    let (rep50_tmpl, rep50_data) = build_repeating_dataset(50);
    cases.push((
        "repeating_50_rows",
        "50 repeating row elements in a collection",
        rep50_tmpl,
        rep50_data,
    ));
    cases.push((
        "wide_table",
        "Wide table: 10 columns × 5 rows",
        TMPL_WIDE.to_string(),
        DATA_WIDE.to_string(),
    ));
    cases.push((
        "tree_50_nodes",
        "Hierarchical tree with 3-level branching (~50 nodes total)",
        TMPL_TREE50.to_string(),
        DATA_TREE50.to_string(),
    ));
    let (mixed50_tmpl, mixed50_data) = build_mixed_types_dataset(50);
    cases.push((
        "mixed_50_types",
        "50 fields of alternating text/numeric types",
        mixed50_tmpl,
        mixed50_data,
    ));

    // ── Unicode ───────────────────────────────────────────────────
    cases.push((
        "arabic_rtl",
        "Arabic RTL text data values",
        TMPL_ARABIC.to_string(),
        DATA_ARABIC.to_string(),
    ));
    cases.push((
        "hebrew_rtl",
        "Hebrew RTL text data values",
        TMPL_HEBREW.to_string(),
        DATA_HEBREW.to_string(),
    ));
    cases.push((
        "cjk_extended",
        "Extended CJK: Traditional Chinese, Japanese kanji, Korean Hangul",
        TMPL_CJK_EXT.to_string(),
        DATA_CJK_EXT.to_string(),
    ));
    cases.push((
        "emoji_unicode",
        "Emoji and symbols via XML numeric character references",
        TMPL_EMOJI.to_string(),
        DATA_EMOJI.to_string(),
    ));
    cases.push((
        "multilingual",
        "Mixed: Latin, Cyrillic, Greek, and CJK in one dataset",
        TMPL_MULTILINGUAL.to_string(),
        DATA_MULTILINGUAL.to_string(),
    ));

    for (i, (suffix, desc, tmpl, data)) in cases.iter().enumerate() {
        let filename = format!("xd_{:02}_{suffix}.pdf", i + 1);
        let path = out_dir.join(&filename);
        let xdp = build_xdp(desc, tmpl, data);
        let bytes = build_pdf(xdp);
        std::fs::write(&path, &bytes).expect("write PDF");
        println!("  {filename}  ({} bytes)  — {desc}", bytes.len());
    }

    println!(
        "\n{} XFA data fixture PDFs written to {}",
        cases.len(),
        out_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Templates  (form structure — fields bound by name to data nodes)
// ---------------------------------------------------------------------------

const TMPL_PERSONAL: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="formData" layout="tb" w="7.5in">
      <field name="firstName" w="4in" h="0.3in">
        <caption><value><text>First Name</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="lastName" w="4in" h="0.3in">
        <caption><value><text>Last Name</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="email" w="5in" h="0.3in">
        <caption><value><text>Email</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="phone" w="3in" h="0.3in">
        <caption><value><text>Phone</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
    </subform>
  </subform>"#;

const DATA_PERSONAL: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <formData>
      <firstName>John</firstName>
      <lastName>Doe</lastName>
      <email>john.doe@example.com</email>
      <phone>+31 20 123 4567</phone>
    </formData>
  </xfa:data>"#;

const TMPL_ADDRESS: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="address" layout="tb" w="7.5in">
      <field name="street" w="6in" h="0.3in">
        <caption><value><text>Street</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="city" w="4in" h="0.3in">
        <caption><value><text>City</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="postalCode" w="2in" h="0.3in">
        <caption><value><text>Postal Code</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <field name="country" w="3in" h="0.3in">
        <caption><value><text>Country</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
    </subform>
  </subform>"#;

const DATA_ADDRESS: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <address>
      <street>Herengracht 182</street>
      <city>Amsterdam</city>
      <postalCode>1016 BS</postalCode>
      <country>Netherlands</country>
    </address>
  </xfa:data>"#;

const TMPL_DATES: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="dates" layout="tb" w="7.5in">
      <field name="birthDate" w="3in" h="0.3in">
        <caption><value><text>Date of Birth</text></value></caption>
        <ui><dateTimeEdit/></ui><value><date/></value>
      </field>
      <field name="issueDate" w="3in" h="0.3in">
        <caption><value><text>Issue Date</text></value></caption>
        <ui><dateTimeEdit/></ui><value><date/></value>
      </field>
      <field name="expiryDate" w="3in" h="0.3in">
        <caption><value><text>Expiry Date</text></value></caption>
        <ui><dateTimeEdit/></ui><value><date/></value>
      </field>
    </subform>
  </subform>"#;

const DATA_DATES: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <dates>
      <birthDate>1985-06-15</birthDate>
      <issueDate>2024-01-01</issueDate>
      <expiryDate>2034-01-01</expiryDate>
    </dates>
  </xfa:data>"#;

const TMPL_ROWS: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="invoice" layout="tb" w="7.5in">
      <field name="invoiceNumber" w="3in" h="0.3in">
        <caption><value><text>Invoice Number</text></value></caption>
        <ui><textEdit/></ui><value><text/></value>
      </field>
      <subform name="rows" layout="tb" w="7.5in">
        <subform name="row" layout="lr-tb" w="7.5in" h="0.3in">
          <field name="product" w="3in" h="0.3in">
            <ui><textEdit/></ui><value><text/></value>
          </field>
          <field name="qty" w="1.5in" h="0.3in">
            <ui><numericEdit/></ui><value><float>0</float></value>
          </field>
          <field name="price" w="2in" h="0.3in">
            <ui><numericEdit/></ui><value><float>0</float></value>
          </field>
        </subform>
      </subform>
    </subform>
  </subform>"#;

// Three row elements — siblings with the same name; DataDom keeps the last value
// per path, but the roundtrip is still consistent (original and roundtrip both keep last).
const DATA_ROWS: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <invoice>
      <invoiceNumber>INV-2024-001</invoiceNumber>
      <rows>
        <row><product>Widget A</product><qty>10</qty><price>5.00</price></row>
        <row><product>Widget B</product><qty>5</qty><price>12.00</price></row>
        <row><product>Widget C</product><qty>20</qty><price>3.50</price></row>
      </rows>
    </invoice>
  </xfa:data>"#;

const TMPL_COMBINED: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="formData" layout="tb" w="7.5in">
      <field name="firstName" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="lastName" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="street" w="6in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="city" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="postalCode" w="2in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="country" w="3in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="birthDate" w="3in" h="0.3in">
        <ui><dateTimeEdit/></ui><value><date/></value></field>
    </subform>
  </subform>"#;

const DATA_COMBINED: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <formData>
      <firstName>Maria</firstName>
      <lastName>Garcia</lastName>
      <street>Calle Mayor 42</street>
      <city>Madrid</city>
      <postalCode>28013</postalCode>
      <country>Spain</country>
      <birthDate>1990-03-22</birthDate>
    </formData>
  </xfa:data>"#;

const TMPL_NESTED: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="org" layout="tb" w="7.5in">
      <field name="companyName" w="5in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <subform name="department" layout="tb" w="7in">
        <field name="deptName" w="4in" h="0.3in">
          <ui><textEdit/></ui><value><text/></value></field>
        <subform name="employee" layout="tb" w="6in">
          <field name="empName" w="4in" h="0.3in">
            <ui><textEdit/></ui><value><text/></value></field>
          <field name="empRole" w="4in" h="0.3in">
            <ui><textEdit/></ui><value><text/></value></field>
        </subform>
      </subform>
    </subform>
  </subform>"#;

const DATA_NESTED: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <org>
      <companyName>Acme Corporation</companyName>
      <department>
        <deptName>Engineering</deptName>
        <employee>
          <empName>Alice Smith</empName>
          <empRole>Senior Engineer</empRole>
        </employee>
      </department>
    </org>
  </xfa:data>"#;

const TMPL_UNICODE: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="uniForm" layout="tb" w="7.5in">
      <field name="nameDE" w="5in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="nameFR" w="5in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="nameNL" w="5in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="currency" w="3in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_UNICODE: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <uniForm>
      <nameDE>Müller Straße 5</nameDE>
      <nameFR>Château d'Amboise</nameFR>
      <nameNL>Keizersgracht 123</nameNL>
      <currency>42.50 EUR</currency>
    </uniForm>
  </xfa:data>"#;

const TMPL_NUMERIC: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="nums" layout="tb" w="7.5in">
      <field name="count" w="3in" h="0.3in">
        <ui><numericEdit/></ui><value><integer>0</integer></value></field>
      <field name="amount" w="3in" h="0.3in">
        <ui><numericEdit/></ui><value><float>0</float></value></field>
      <field name="rate" w="3in" h="0.3in">
        <ui><numericEdit/></ui><value><float>0</float></value></field>
      <field name="total" w="3in" h="0.3in">
        <ui><numericEdit/></ui><value><float>0</float></value></field>
    </subform>
  </subform>"#;

const DATA_NUMERIC: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <nums>
      <count>42</count>
      <amount>1234.56</amount>
      <rate>0.21</rate>
      <total>1493.82</total>
    </nums>
  </xfa:data>"#;

const TMPL_MULTIPAGE: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
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
    <subform name="mpData" layout="tb" w="7.5in">
      <field name="f1" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f2" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f3" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f4" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f5" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f6" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f7" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f8" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f9" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="f10" w="7in" h="0.4in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_MULTIPAGE: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <mpData>
      <f1>Page one field one</f1>
      <f2>Page one field two</f2>
      <f3>Page one field three</f3>
      <f4>Overflow to page two - field four</f4>
      <f5>Field five</f5>
      <f6>Field six</f6>
      <f7>Field seven</f7>
      <f8>Field eight</f8>
      <f9>Field nine</f9>
      <f10>Field ten</f10>
    </mpData>
  </xfa:data>"#;

const TMPL_MIXED: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="mixedForm" layout="tb" w="7.5in">
      <field name="filled1" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="empty1" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="filled2" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="empty2" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
      <field name="filled3" w="4in" h="0.3in">
        <ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// empty1 and empty2 are deliberately left out of the data so those
// fields have no bound value. Only filled1/2/3 have leaf values.
const DATA_MIXED: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <mixedForm>
      <filled1>Alpha value</filled1>
      <filled2>Beta value</filled2>
      <filled3>Gamma value</filled3>
    </mixedForm>
  </xfa:data>"#;

// ---------------------------------------------------------------------------
// Edge case templates and data
// ---------------------------------------------------------------------------

const TMPL_NULL_VALUES: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="nullForm" layout="tb" w="7.5in">
      <field name="presentField" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="emptyField" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="nilField" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="anotherPresent" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// emptyField has empty text; nilField uses xsi:nil; both should roundtrip correctly.
const DATA_NULL_VALUES: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <nullForm>
      <presentField>I have a value</presentField>
      <emptyField></emptyField>
      <nilField xsi:nil="true"/>
      <anotherPresent>Also present</anotherPresent>
    </nullForm>
  </xfa:data>"#;

const TMPL_DEEP: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="level1" layout="tb" w="7.5in">
      <field name="topField" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// 5-level deep nesting: data.level1.level2.level3.level4.level5.deepValue
const DATA_DEEP: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <level1>
      <topField>top</topField>
      <level2>
        <l2Field>level 2</l2Field>
        <level3>
          <l3Field>level 3</l3Field>
          <level4>
            <l4Field>level 4</l4Field>
            <level5>
              <deepValue>deep leaf at level 5</deepValue>
            </level5>
          </level4>
        </level3>
      </level2>
    </level1>
  </xfa:data>"#;

const TMPL_CJK: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="cjkForm" layout="tb" w="7.5in">
      <field name="japanese" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="chinese" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="korean" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="emoji" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="mixed" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// Emoji via XML numeric character references (safe in XML 1.0).
const DATA_CJK: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <cjkForm>
      <japanese>こんにちは世界</japanese>
      <chinese>你好世界</chinese>
      <korean>안녕하세요</korean>
      <emoji>&#x1F600; &#x1F4C4; &#x2705;</emoji>
      <mixed>PDF &#x1F4C4; XFA &#x2022; FormCalc</mixed>
    </cjkForm>
  </xfa:data>"#;

const TMPL_EMPTY_COLL: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="collForm" layout="tb" w="7.5in">
      <field name="title" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// items element has no children: empty collection with 0 items.
// DataDom will see it as a DataValue with empty text.
const DATA_EMPTY_COLL: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <collForm>
      <title>Empty collection test</title>
      <items></items>
      <count>0</count>
    </collForm>
  </xfa:data>"#;

/// Build template and data for a large dataset (n fields).
fn build_large_dataset(n: usize) -> (String, String) {
    let field_defs: String = (1..=n)
        .map(|i| {
            format!(
                r#"      <field name="f{i:03}" w="7in" h="0.25in">
        <ui><textEdit/></ui><value><text/></value>
      </field>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let tmpl = format!(
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
    <subform name="bigForm" layout="tb" w="7.5in">
{field_defs}
    </subform>
  </subform>"#
    );

    let data_fields: String = (1..=n)
        .map(|i| format!("      <f{i:03}>Value {i:03}</f{i:03}>"))
        .collect::<Vec<_>>()
        .join("\n");

    let data = format!(
        r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <bigForm>
{data_fields}
    </bigForm>
  </xfa:data>"#
    );

    (tmpl, data)
}

// ---------------------------------------------------------------------------
// Edge case templates and data (new)
// ---------------------------------------------------------------------------

const TMPL_NULL_NESTED: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="report" layout="tb" w="7.5in">
      <field name="title" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <subform name="section" layout="tb" w="7in">
        <field name="sectionHead" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="sectionBody" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="sectionNote" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      </subform>
    </subform>
  </subform>"#;

// sectionBody and sectionNote intentionally absent from data (null by omission)
const DATA_NULL_NESTED: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <report>
      <title>Null nesting test</title>
      <section>
        <sectionHead>Present heading</sectionHead>
      </section>
    </report>
  </xfa:data>"#;

const TMPL_EMPTY_ARR: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="container" layout="tb" w="7.5in">
      <field name="label" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="count" w="2in" h="0.3in"><ui><numericEdit/></ui><value><integer>0</integer></value></field>
    </subform>
  </subform>"#;

const DATA_EMPTY_ARR: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <container>
      <label>Empty collection</label>
      <count>0</count>
      <items></items>
      <tags></tags>
    </container>
  </xfa:data>"#;

const TMPL_SIX_DEEP: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="root" layout="tb" w="7.5in">
      <field name="rootVal" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_SIX_DEEP: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <root>
      <rootVal>level 1</rootVal>
      <l2>
        <l2v>level 2</l2v>
        <l3>
          <l3v>level 3</l3v>
          <l4>
            <l4v>level 4</l4v>
            <l5>
              <l5v>level 5</l5v>
              <l6>
                <deepest>leaf at level 6</deepest>
              </l6>
            </l5>
          </l4>
        </l3>
      </l2>
    </root>
  </xfa:data>"#;

const TMPL_XML_CHARS: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="xmlChars" layout="tb" w="7.5in">
      <field name="ampField" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="ltGtField" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="quoteField" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="mixedField" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_XML_CHARS: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xmlChars>
      <ampField>Tom &amp; Jerry Productions</ampField>
      <ltGtField>x &lt; 10 &amp;&amp; y &gt; 0</ltGtField>
      <quoteField>He said &quot;Hello&quot; and she said &apos;Hi&apos;</quoteField>
      <mixedField>A&amp;B &lt;tag&gt; &quot;quoted&quot;</mixedField>
    </xmlChars>
  </xfa:data>"#;

const TMPL_SIBLINGS: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="catalog" layout="tb" w="7.5in">
      <field name="name" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <subform name="entries" layout="tb" w="7in">
        <subform name="entry" layout="lr-tb" w="7in" h="0.3in">
          <field name="key" w="3in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
          <field name="value" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        </subform>
      </subform>
    </subform>
  </subform>"#;

const DATA_SIBLINGS: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <catalog>
      <name>Color Map</name>
      <entries>
        <entry><key>red</key><value>#FF0000</value></entry>
        <entry><key>green</key><value>#00FF00</value></entry>
        <entry><key>blue</key><value>#0000FF</value></entry>
        <entry><key>white</key><value>#FFFFFF</value></entry>
        <entry><key>black</key><value>#000000</value></entry>
      </entries>
    </catalog>
  </xfa:data>"#;

// ---------------------------------------------------------------------------
// Wide table constants
// ---------------------------------------------------------------------------

const TMPL_WIDE: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.25in" y="0.5in" w="8in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="wideTable" layout="tb" w="8in">
      <subform name="row1" layout="lr-tb" w="8in" h="0.3in">
        <field name="c1" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c2" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c3" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c4" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c5" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c6" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c7" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c8" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c9" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
        <field name="c10" w="0.8in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      </subform>
    </subform>
  </subform>"#;

const DATA_WIDE: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <wideTable>
      <row1>
        <c1>Col1R1</c1><c2>Col2R1</c2><c3>Col3R1</c3><c4>Col4R1</c4><c5>Col5R1</c5>
        <c6>Col6R1</c6><c7>Col7R1</c7><c8>Col8R1</c8><c9>Col9R1</c9><c10>Col10R1</c10>
      </row1>
    </wideTable>
  </xfa:data>"#;

// ---------------------------------------------------------------------------
// Hierarchical tree constants
// ---------------------------------------------------------------------------

const TMPL_TREE50: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="org" layout="tb" w="7.5in">
      <field name="orgName" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_TREE50: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <org>
      <orgName>Acme Corp</orgName>
      <div><name>Engineering</name>
        <team><name>Backend</name><member>Alice</member><member>Bob</member><member>Carol</member></team>
        <team><name>Frontend</name><member>Dave</member><member>Eve</member><member>Frank</member></team>
        <team><name>DevOps</name><member>Grace</member><member>Hank</member><member>Ivy</member></team>
      </div>
      <div><name>Product</name>
        <team><name>Design</name><member>Jack</member><member>Kate</member><member>Leo</member></team>
        <team><name>Research</name><member>Mia</member><member>Noah</member><member>Olivia</member></team>
      </div>
      <div><name>Operations</name>
        <team><name>Finance</name><member>Paul</member><member>Quinn</member><member>Rose</member></team>
        <team><name>HR</name><member>Sam</member><member>Tina</member><member>Uma</member></team>
        <team><name>Legal</name><member>Victor</member><member>Wendy</member><member>Xena</member></team>
      </div>
    </org>
  </xfa:data>"#;

// ---------------------------------------------------------------------------
// Unicode constants
// ---------------------------------------------------------------------------

const TMPL_ARABIC: &str = r#"<subform name="form1" layout="paginate" locale="ar_AE">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="arabicForm" layout="tb" w="7.5in">
      <field name="greeting" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="city" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="country" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="phrase" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_ARABIC: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <arabicForm>
      <greeting>مرحبا بالعالم</greeting>
      <city>دبي</city>
      <country>الإمارات العربية المتحدة</country>
      <phrase>نموذج XFA بالعربية</phrase>
    </arabicForm>
  </xfa:data>"#;

const TMPL_HEBREW: &str = r#"<subform name="form1" layout="paginate" locale="he_IL">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="hebrewForm" layout="tb" w="7.5in">
      <field name="greeting" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="city" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="label" w="5in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_HEBREW: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <hebrewForm>
      <greeting>שלום עולם</greeting>
      <city>תל אביב</city>
      <label>טופס XFA בעברית</label>
    </hebrewForm>
  </xfa:data>"#;

const TMPL_CJK_EXT: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="cjkExt" layout="tb" w="7.5in">
      <field name="tradChinese" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="japanese" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="korean" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="mixed" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_CJK_EXT: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <cjkExt>
      <tradChinese>繁體中文表單填寫範例</tradChinese>
      <japanese>日本語のXFAフォーム記入例</japanese>
      <korean>한국어 XFA 양식 작성 예시</korean>
      <mixed>PDF・XFA・FormCalc 三合一</mixed>
    </cjkExt>
  </xfa:data>"#;

const TMPL_EMOJI: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="emojiForm" layout="tb" w="7.5in">
      <field name="faces" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="objects" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="symbols" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="flags" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

// Emoji as XML numeric character references (safe in XML 1.0)
const DATA_EMOJI: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <emojiForm>
      <faces>&#x1F600; &#x1F604; &#x1F622; &#x1F914; &#x1F389;</faces>
      <objects>&#x1F4C4; &#x1F4BB; &#x1F4F1; &#x1F5C2; &#x1F4BE;</objects>
      <symbols>&#x2705; &#x274C; &#x2B50; &#x2714; &#x2716;</symbols>
      <flags>Status: &#x2705; passed &#x2714; verified</flags>
    </emojiForm>
  </xfa:data>"#;

const TMPL_MULTILINGUAL: &str = r#"<subform name="form1" layout="paginate" locale="en_US">
    <pageSet><pageArea name="Page1" id="Page1">
      <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      <medium stock="default" short="8.5in" long="11in"/>
    </pageArea></pageSet>
    <subform name="multiLang" layout="tb" w="7.5in">
      <field name="latin" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="cyrillic" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="greek" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="cjk" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
      <field name="allTogether" w="6in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
    </subform>
  </subform>"#;

const DATA_MULTILINGUAL: &str = r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <multiLang>
      <latin>Héllo Wörld — Ñoño — Ångström</latin>
      <cyrillic>Привет мир — Москва — Россия</cyrillic>
      <greek>Γεια σου κόσμε — Αθήνα</greek>
      <cjk>你好世界 — こんにちは — 안녕하세요</cjk>
      <allTogether>Hello Привет 你好 Γεια مرحبا שלום</allTogether>
    </multiLang>
  </xfa:data>"#;

// ---------------------------------------------------------------------------
// Dynamic large-data builders
// ---------------------------------------------------------------------------

/// Build template and data for a dataset with alternating text/numeric fields.
fn build_mixed_types_dataset(n: usize) -> (String, String) {
    let field_defs: String = (1..=n)
        .map(|i| {
            if i % 2 == 0 {
                format!(
                    r#"      <field name="n{i:03}" w="7in" h="0.25in">
        <ui><numericEdit/></ui><value><float>0</float></value>
      </field>"#
                )
            } else {
                format!(
                    r#"      <field name="t{i:03}" w="7in" h="0.25in">
        <ui><textEdit/></ui><value><text/></value>
      </field>"#
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let tmpl = format!(
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
    <subform name="mixedForm" layout="tb" w="7.5in">
{field_defs}
    </subform>
  </subform>"#
    );

    let data_fields: String = (1..=n)
        .map(|i| {
            if i % 2 == 0 {
                format!("      <n{i:03}>{}</n{i:03}>", i * 7)
            } else {
                format!("      <t{i:03}>Text value {i:03}</t{i:03}>")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let data = format!(
        r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <mixedForm>
{data_fields}
    </mixedForm>
  </xfa:data>"#
    );

    (tmpl, data)
}

/// Build template and data for n repeating row elements.
fn build_repeating_dataset(n: usize) -> (String, String) {
    let tmpl = r#"<subform name="form1" layout="paginate" locale="en_US">
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
    <subform name="list" layout="tb" w="7.5in">
      <subform name="rows" layout="tb" w="7.5in">
        <subform name="row" layout="lr-tb" w="7.5in" h="0.3in">
          <field name="idx" w="1in" h="0.3in"><ui><numericEdit/></ui><value><integer>0</integer></value></field>
          <field name="label" w="4in" h="0.3in"><ui><textEdit/></ui><value><text/></value></field>
          <field name="amount" w="2in" h="0.3in"><ui><numericEdit/></ui><value><float>0</float></value></field>
        </subform>
      </subform>
    </subform>
  </subform>"#
        .to_string();

    let rows: String = (1..=n)
        .map(|i| {
            format!(
                "        <row><idx>{i}</idx><label>Row {i:03}</label><amount>{:.2}</amount></row>",
                i as f64 * 1.5
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let data = format!(
        r#"<xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <list>
      <rows>
{rows}
      </rows>
    </list>
  </xfa:data>"#
    );

    (tmpl, data)
}

// ---------------------------------------------------------------------------
// XDP + PDF builders
// ---------------------------------------------------------------------------

fn build_xdp(description: &str, template_body: &str, data_body: &str) -> String {
    let desc_safe = description.replace('"', "&quot;").replace('<', "&lt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <desc>
    <text name="description">{desc_safe}</text>
  </desc>
  {template_body}
</template>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  {data_body}
</xfa:datasets>
</xdp:xdp>
"#
    )
}

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
