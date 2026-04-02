# Forms Recipes

## Fill a Form Field

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let mut doc = sdk.open("form.pdf")?;

// Fill by field name
doc.set_field_value("name", "John Doe")?;
doc.set_field_value("email", "john@example.com")?;
doc.set_field_value("amount", "199.99")?;

// Save filled form (still editable)
doc.save("form_filled.pdf")?;
```

---

## Flatten an XFA Form

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let mut doc = sdk.open("xfa_form.xdp")?;

// Pre-fill with data if needed
let xml_data = std::fs::read("form_data.xml")?;
doc.import_xfa_data(&xml_data)?;

// Flatten — converts dynamic form to static PDF
let flat = doc.flatten_xfa()?;
flat.save("form_flat.pdf")?;

println!("Flattened {} pages", flat.page_count());
```

---

## Extract XFA Form Data

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("xfa_form.pdf")?;

let form_data = doc.extract_xfa_data()?;

for (key, value) in form_data.fields {
    println!("{}: {}", key, value);
}

// Or export as XML
let xml = doc.export_xfa_data_xml()?;
std::fs::write("form_data.xml", &xml)?;
```

---

## Execute FormCalc Expressions

```rust
use pdfluent::Sdk;

let sdk = Sdk::init_with_license("license.json")?;
let doc = sdk.open("calculator.xfa")?;

// Evaluate FormCalc expressions
let result = doc.evaluate_formcalc("SUM(field1, field2, field3)")?;
println!("Sum: {}", result);

let tax = doc.evaluate_formcalc("Subtotal * 0.21")?;
println!("Tax: {}", tax);

// Set the calculated value back to a field
doc.set_field_value("total", &tax.to_string())?;
```
