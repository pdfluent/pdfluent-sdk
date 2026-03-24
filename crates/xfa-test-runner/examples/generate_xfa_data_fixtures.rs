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

use std::path::PathBuf;

use lopdf::{Document, Object, Stream, dictionary};

fn main() {
    let out_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xfa-data");
    std::fs::create_dir_all(&out_dir).expect("create xfa-data fixtures dir");

    // Static cases: (suffix, description, template_body, data_body)
    let mut cases: Vec<(&str, &str, String, String)> = vec![
        ("personal_info", "Personal info: name, email, phone",
            TMPL_PERSONAL.to_string(), DATA_PERSONAL.to_string()),
        ("address", "Address fields: street, city, postal code, country",
            TMPL_ADDRESS.to_string(), DATA_ADDRESS.to_string()),
        ("date_fields", "Date fields: birth date, issue date, expiry date",
            TMPL_DATES.to_string(), DATA_DATES.to_string()),
        ("repeating_rows", "Repeating rows: 3 invoice line items",
            TMPL_ROWS.to_string(), DATA_ROWS.to_string()),
        ("combined", "Combined: personal + address + dates",
            TMPL_COMBINED.to_string(), DATA_COMBINED.to_string()),
        ("nested_groups", "Nested groups: company → department → employee",
            TMPL_NESTED.to_string(), DATA_NESTED.to_string()),
        ("unicode_values", "Unicode values: accented chars and symbols",
            TMPL_UNICODE.to_string(), DATA_UNICODE.to_string()),
        ("numeric_values", "Numeric values: integers and decimals",
            TMPL_NUMERIC.to_string(), DATA_NUMERIC.to_string()),
        ("multipage_data", "Multi-page: 10 data-bound fields",
            TMPL_MULTIPAGE.to_string(), DATA_MULTIPAGE.to_string()),
        ("mixed_empty", "Mix: some fields filled, some empty",
            TMPL_MIXED.to_string(), DATA_MIXED.to_string()),
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
