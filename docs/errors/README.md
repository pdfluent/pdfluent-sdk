# PDFluent Error Reference

> Complete list of error codes returned by PDFluent SDK with explanations, causes, and solutions.

## Error Code Format

Each error follows the format: `EXXX`

| Code | Category |
|------|----------|
| E100-E199 | File/IO Errors |
| E200-E299 | PDF Structure Errors |
| E300-E399 | XFA Errors |
| E400-E499 | Cryptography/Signature Errors |
| E500-E599 | Compliance/Validation Errors |

## Quick Reference

| Code | Error | Likely Cause |
|------|-------|--------------|
| E101 | File not found | Wrong path or file doesn't exist |
| E102 | Permission denied | No read access to file |
| E201 | Invalid PDF structure | Corrupted or non-PDF file |
| E301 | XFA not found | PDF doesn't contain XFA forms |
| E302 | XFA parsing failed | Malformed XFA content |
| E401 | Invalid signature | Certificate or signature corrupted |
| E501 | PDF/A validation failed | Document doesn't meet PDF/A requirements |

---

## Individual Error Pages

- [E101: File Not Found](./E101.md)
- [E102: Permission Denied](./E102.md)
- [E201: Invalid PDF Structure](./E201.md)
- [E202: Encrypted PDF](./E202.md)
- [E301: XFA Not Found](./E301.md)
- [E302: XFA Parsing Failed](./E302.md)
- [E401: Invalid Signature](./E401.md)
- [E501: PDF/A Validation Failed](./E501.md)

---

*Last updated: April 2026*
*Tracking issue: https://github.com/jasperdew/xfa-native-rust/issues/612*
