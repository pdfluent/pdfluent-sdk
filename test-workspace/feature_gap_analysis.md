# XFA Feature Gap Analysis (Issue #755)

Analysis of 15 passing (SSIM ≥ 0.99) and 15 failing (SSIM < 0.80) XFA entries from the Gate7 Golden Set.

## 1. Feature Matrix Summary

| Group | Avg. Fields | Max Depth | Avg. Scripts | Data Binding | Choice Lists | Barcodes |
|-------|-------------|-----------|--------------|--------------|--------------|----------|
| **PASS** | 0.6 | 2.4 | 0.1 | 0% | 0% | 0% |
| **FAIL** | 71.3 | 10.3 | 48.9 | 86% | 73% | 20% |

### Key Observations:
- **Passing entries** are mostly trivial or "empty" XFA shells with almost no fields or logic.
- **Failing entries** are real-world complex forms with high field counts and heavy scripting.

## 2. Top 3 Features Causing Failure

### 1. Missing Choice List (Dropdown) Support (Impact: ~75% of failing entries)
Failing entries use an average of 30+ choice lists per form (max 69). The current `FormNode::Field` structure only stores a single `value: String` and completely ignores the `<items>` element. This prevents dropdowns from rendering correctly as they have no list data to show.

### 2. Scripting Complexity (FormCalc/JS) (Impact: ~90% of failing entries)
Failing forms have hundreds of scripts (up to 207 JS / 165 FormCalc). Many of these scripts control visibility (`presence="hidden"`) or dynamic layout. If the scripting engine or event model is incomplete, the layout engine renders elements that should be hidden, leading to massive SSIM drops.

### 3. Deep Nesting & Data Binding (Impact: ~85% of failing entries)
Failing entries consistently have a nesting depth of 10-13 levels and use extensive data binding (`ref` or `bind`). Passing entries have low depth (max 6) and no data binding. This suggests that the SOM (Scripting Object Model) resolution and Data DOM merging might fail on deeply nested structures.

## 3. Concrete Recommendations

1. **Implement Choice List Support:**
   - Extend `FormNodeType::Field` in `xfa-layout-engine` to include an `items: Vec<String>` field.
   - Update the XFA template parser to extract `<items>` from `<field>` nodes.
   - Update the renderer to display the selected item from the list.

2. **Harden Scripting & Event Model:**
   - Prioritize `presence` property updates via scripts.
   - Ensure `calculate` and `validate` events are fired in the correct order during the merge phase.
   - Add support for common JavaScript XFA objects that are currently missing.

3. **Improve SOM Resolution for Deep Nesting:**
   - Test and fix SOM path resolution for paths with 10+ segments.
   - Ensure data binding correctly handles relative paths (`$.data...`) in deeply nested subforms.

4. **Add Barcode & Image support:**
   - Some failing forms contain `<barcode>` and `<image>` elements which are currently poorly handled or ignored.

## 4. Feature Matrix (Full Data)

| Hash (Prefix) | Pass/Fail | SSIM | Fields | Depth | Scripts | Data Bind | Choice Lists |
|---------------|-----------|------|--------|-------|---------|-----------|--------------|
| 59200f99 | PASS | 1.00 | 0 | 0 | 0 | 0 | 0 |
| eda82752 | PASS | 1.00 | 6 | 6 | 0 | 0 | 0 |
| 04b84f40 | PASS | 1.00 | 1 | 6 | 1 | 0 | 0 |
| db734985 | FAIL | <0.80| 24 | 11 | 5 | 9 | 1 |
| bd77b716 | FAIL | <0.80| 71 | 11 | 165 | 0 | 55 |
| 1bb4a6b0 | FAIL | <0.80| 143| 10 | 207 | 54 | 69 |
| 48244056 | FAIL | <0.80| 105| 10 | 49 | 6 | 61 |
| ee60923a | FAIL | <0.80| 136| 13 | 161 | 206| 32 |
| ... | ... | ... | ... | ... | ... | ... | ... |

*(Full CSV data available in test-workspace/analysis_pass.csv and analysis_fail.csv)*
