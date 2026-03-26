# pdf-engine

Unified PDF rendering engine for rendering, text extraction, thumbnails, and OCR integration.

## OCR Cloud Providers

| Provider | Feature | Env Vars | Auth |
|----------|---------|----------|------|
| Mistral | `ocr-mistral` | `MISTRAL_API_KEY` | API key |
| Google Vision | `ocr-google` | `GOOGLE_VISION_API_KEY` or `GOOGLE_APPLICATION_CREDENTIALS` | API key or service account |
| AWS Textract | `ocr-aws` | `AWS_REGION` + `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` | IAM credentials |
| Azure Doc Intel | `ocr-azure` | `AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT` + `AZURE_DOCUMENT_INTELLIGENCE_KEY` | API key |

Enable all hosted OCR adapters together with:

```bash
cargo test -p pdf-engine --features ocr-cloud
```

Notes:

- `best_available_backend()` prefers cloud providers in this order: Mistral, Google Vision, Azure Document Intelligence, AWS Textract, then local OCR fallbacks.
- Google auto-detection checks `GOOGLE_VISION_API_KEY` first, then `GOOGLE_APPLICATION_CREDENTIALS`, then gcloud application-default credentials when `GOOGLE_CLOUD_PROJECT` is set.
- Azure uses API version `2024-11-30` by default. Override with `AZURE_DOCUMENT_INTELLIGENCE_API_VERSION` if your deployment requires a different version.
