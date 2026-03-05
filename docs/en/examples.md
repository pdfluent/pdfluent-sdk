# Code Examples

## Rust

### Extract and Print All Fields

```rust
use pdfium_ffi_bridge::pipeline::extract_xfa_from_file;
use pdfium_ffi_bridge::template_parser::parse_template;
use xfa_json::form_tree_to_json;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let packets = extract_xfa_from_file(Path::new("form.pdf"))?;
    let template_xml = packets.template().expect("no template packet");
    let (tree, root) = parse_template(template_xml, packets.datasets())?;
    let data = form_tree_to_json(&tree, root);

    for (path, value) in &data.fields {
        println!("{path}: {value:?}");
    }
    Ok(())
}
```

### Fill Form and Save PDF

```rust
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::template_parser::parse_template;
use xfa_json::{json_to_form_tree, FormData, FieldValue};
use indexmap::IndexMap;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut reader = PdfReader::from_file(Path::new("form.pdf"))?;
    let packets = reader.extract_xfa()?;
    let template_xml = packets.template().expect("no template packet");
    let (mut tree, root) = parse_template(template_xml, packets.datasets())?;

    let mut fields = IndexMap::new();
    fields.insert("form1.Name".into(), FieldValue::Text("Jane Smith".into()));
    fields.insert("form1.Amount".into(), FieldValue::Number(2500.0));
    let data = FormData { fields };

    json_to_form_tree(&data, &mut tree, root);
    reader.save_to_file(Path::new("filled.pdf"))?;
    Ok(())
}
```

## Python (via REST API)

### Extract Fields

```python
import requests

with open("form.pdf", "rb") as f:
    response = requests.post(
        "http://localhost:3000/extract",
        files={"file": f}
    )

data = response.json()
for path, value in data["fields"].items():
    print(f"{path}: {value}")
```

### Fill Form

```python
import requests
import json

with open("form.pdf", "rb") as f:
    response = requests.post(
        "http://localhost:3000/fill",
        files={"file": f},
        data={"data": json.dumps({
            "form1.Name": "Jane Smith",
            "form1.Amount": 2500.0
        })}
    )

with open("filled.pdf", "wb") as out:
    out.write(response.content)
```

### Batch Processing

```python
import requests
from pathlib import Path

pdf_dir = Path("forms/")
for pdf_file in pdf_dir.glob("*.pdf"):
    with open(pdf_file, "rb") as f:
        resp = requests.post("http://localhost:3000/extract", files={"file": f})

    if resp.ok:
        data = resp.json()
        print(f"{pdf_file.name}: {len(data['fields'])} fields")
    else:
        print(f"{pdf_file.name}: ERROR - {resp.json()['error']}")
```

## JavaScript / Node.js

### Extract Fields

```javascript
const fs = require("fs");
const FormData = require("form-data");
const axios = require("axios");

async function extractFields(pdfPath) {
  const form = new FormData();
  form.append("file", fs.createReadStream(pdfPath));

  const { data } = await axios.post("http://localhost:3000/extract", form, {
    headers: form.getHeaders(),
  });

  return data.fields;
}

extractFields("form.pdf").then((fields) => {
  Object.entries(fields).forEach(([path, value]) => {
    console.log(`${path}: ${value}`);
  });
});
```

### Fill and Download

```javascript
const fs = require("fs");
const FormData = require("form-data");
const axios = require("axios");

async function fillForm(pdfPath, fieldValues, outputPath) {
  const form = new FormData();
  form.append("file", fs.createReadStream(pdfPath));
  form.append("data", JSON.stringify(fieldValues));

  const { data } = await axios.post("http://localhost:3000/fill", form, {
    headers: form.getHeaders(),
    responseType: "arraybuffer",
  });

  fs.writeFileSync(outputPath, Buffer.from(data));
}

fillForm("form.pdf", { "form1.Name": "Jane" }, "filled.pdf");
```

## C# / .NET

### Extract Fields

```csharp
using System.Net.Http;
using System.Text.Json;

async Task<Dictionary<string, JsonElement>> ExtractFields(string pdfPath)
{
    using var client = new HttpClient();
    using var content = new MultipartFormDataContent();
    content.Add(new StreamContent(File.OpenRead(pdfPath)), "file", Path.GetFileName(pdfPath));

    var response = await client.PostAsync("http://localhost:3000/extract", content);
    response.EnsureSuccessStatusCode();

    var json = await response.Content.ReadAsStringAsync();
    var doc = JsonDocument.Parse(json);
    var fields = new Dictionary<string, JsonElement>();

    foreach (var prop in doc.RootElement.GetProperty("fields").EnumerateObject())
    {
        fields[prop.Name] = prop.Value;
    }
    return fields;
}
```

### Fill Form

```csharp
async Task FillForm(string pdfPath, Dictionary<string, object> data, string outputPath)
{
    using var client = new HttpClient();
    using var content = new MultipartFormDataContent();
    content.Add(new StreamContent(File.OpenRead(pdfPath)), "file", Path.GetFileName(pdfPath));
    content.Add(new StringContent(JsonSerializer.Serialize(data)), "data");

    var response = await client.PostAsync("http://localhost:3000/fill", content);
    response.EnsureSuccessStatusCode();

    await using var fs = File.Create(outputPath);
    await response.Content.CopyToAsync(fs);
}
```

## Java

### Extract Fields

```java
import java.net.http.*;
import java.nio.file.Path;

public class XfaExtractor {
    private static final HttpClient client = HttpClient.newHttpClient();

    public static String extractFields(String pdfPath) throws Exception {
        var boundary = "----XfaBoundary";
        var body = buildMultipartBody(pdfPath, boundary);

        var request = HttpRequest.newBuilder()
            .uri(URI.create("http://localhost:3000/extract"))
            .header("Content-Type", "multipart/form-data; boundary=" + boundary)
            .POST(HttpRequest.BodyPublishers.ofByteArray(body))
            .build();

        var response = client.send(request, HttpResponse.BodyHandlers.ofString());
        return response.body();
    }

    private static byte[] buildMultipartBody(String filePath, String boundary)
        throws Exception
    {
        var fileBytes = java.nio.file.Files.readAllBytes(Path.of(filePath));
        var fileName = Path.of(filePath).getFileName().toString();

        var sb = new StringBuilder();
        sb.append("--").append(boundary).append("\r\n");
        sb.append("Content-Disposition: form-data; name=\"file\"; filename=\"")
          .append(fileName).append("\"\r\n");
        sb.append("Content-Type: application/pdf\r\n\r\n");

        var header = sb.toString().getBytes();
        var footer = ("\r\n--" + boundary + "--\r\n").getBytes();

        var result = new byte[header.length + fileBytes.length + footer.length];
        System.arraycopy(header, 0, result, 0, header.length);
        System.arraycopy(fileBytes, 0, result, header.length, fileBytes.length);
        System.arraycopy(footer, 0, result, header.length + fileBytes.length, footer.length);
        return result;
    }
}
```

## cURL

### Quick Commands

```bash
# Extract fields
curl -s -X POST http://localhost:3000/extract \
  -F "file=@form.pdf" | jq '.fields'

# Get schema
curl -s -X POST http://localhost:3000/schema \
  -F "file=@form.pdf" | jq '.fields | keys'

# Fill form
curl -X POST http://localhost:3000/fill \
  -F "file=@form.pdf" \
  -F 'data={"form1.Name":"Jane","form1.Amount":2500}' \
  -o filled.pdf

# Flatten to AcroForm
curl -X POST http://localhost:3000/flatten \
  -F "file=@form.pdf" \
  -o flat.pdf

# Validate
curl -s -X POST http://localhost:3000/validate \
  -F "file=@form.pdf" | jq .
```
