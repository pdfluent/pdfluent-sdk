# Compliance Test Suite Baseline — 2026-03-08

## Test Suites Downloaded
| Suite    | Files | Source |
|----------|-------|--------|
| VeraPDF  | 2702  | github.com/veraPDF/veraPDF-corpus |
| Isartor  | 205   | pdfa.org (via VeraPDF corpus) |
| BFO      | 33    | github.com/bfosupport/pdfa-testsuite |
| **Total**| **2940** | |

Ground truth: 1849 expected-fail, 1080 expected-pass, 11 unknown.
Tested: 1888 (skipped 1052: PDF/A-4, PDF/UA, unknown profiles).

## Baseline Accuracy
| Suite    | Correct | Total | Accuracy |
|----------|---------|-------|----------|
| VeraPDF  | 757     | 1650  | 45.8%    |
| Isartor  | 141     | 205   | 68.7%    |
| BFO      | 11      | 33    | 33.3%    |
| **Total**| **909** | **1888** | **48.1%** |

- False positives (we pass, should fail): 922
- False negatives (we fail, should pass): 56
- Errors/timeouts: 0

## Top Mismatched Clauses
| Clause  | Count | Description |
|---------|-------|-------------|
| 6.6.2.3 | 293  | Named action restrictions |
| 6.7.2   | 195  | XMP metadata properties |
| 6.2.3.3 | 31   | Device color vs OutputIntent |
| 6.3.3   | 27   | Font embedding |
| 6.3.2   | 26   | Annotation flag requirements |
| 6.2.4.3 | 18   | Device color space defaults |
| 6.1.12  | 18   | Implementation limits |
| 6.7.9   | 16   | XMP extension schemas |
| 6.1.13  | 16   | Page boundaries |
| 6.2.11.7| 15   | CIDFont restrictions |
| 6.1.2   | 15   | File header |
| 6.5.1   | 14   | Annotation flags deep |
| 6.5.3   | 13   | Annotation appearance |
| 6.5.2   | 13   | Annotation subtypes |
| 6.2.2   | 13   | OutputIntent requirements |

## Next Steps
- Focus on top false positive clauses (6.6.2.3, 6.7.2) for biggest accuracy gains
- Target: 80%+ accuracy on VeraPDF + Isartor suites
