import os
import subprocess
import json
import time
import sys
from collections import Counter

CONVERT_BIN = "target/release/examples/convert_pdfa"
VALIDATE_BIN = "target/release/xfa-cli"
TIMEOUT = 120

def run_test(pdf_path):
    output_pdf = "/tmp/retest_out.pdf"
    if os.path.exists(output_pdf):
        os.remove(output_pdf)
    
    start_time = time.time()
    try:
        # Conversion
        res = subprocess.run([CONVERT_BIN, pdf_path, output_pdf], 
                             capture_output=True, timeout=TIMEOUT)
        if res.returncode != 0:
            err = res.stderr.decode('utf-8', errors='ignore').strip()
            return "FAIL", err.split('\n')[-1] if err else "Conversion error"
        
        # Validation
        res = subprocess.run([VALIDATE_BIN, "validate", "--json", output_pdf],
                             capture_output=True, timeout=TIMEOUT)
        if res.returncode == 0:
            return "PASS", None
        
        try:
            report = json.loads(res.stdout)
            errors = [i['rule'] + ": " + i['message'] for i in report.get('issues', []) if i.get('severity') == 'Error']
            if not errors:
                errors = [i['rule'] + ": " + i['message'] for i in report.get('issues', [])]
            
            return "FAIL", errors[0] if errors else "Validation failed"
        except Exception as e:
            return "FAIL", f"Failed to parse validation JSON"

    except subprocess.TimeoutExpired:
        return "TIMEOUT", None
    except Exception as e:
        return "FAIL", str(e)
    finally:
        if os.path.exists(output_pdf):
            try: os.remove(output_pdf)
            except: pass

def main():
    if not os.path.exists("retest_list.txt"):
        print("retest_list.txt not found!")
        return

    with open("retest_list.txt", "r") as f:
        paths = [line.strip() for line in f if line.strip()]
    
    results = []
    print(f"Testing {len(paths)} PDFs...")
    
    for i, local_path in enumerate(paths):
        status, detail = run_test(local_path)
        results.append((status, detail))
        print(f"[{i+1}/{len(paths)}] {status} {detail if detail else ''}")
        sys.stdout.flush()

    if not results:
        print("No results collected!")
        return

    # Report
    total = len(results)
    pass_count = sum(1 for r in results if r[0] == "PASS")
    fail_count = sum(1 for r in results if r[0] == "FAIL")
    timeout_count = sum(1 for r in results if r[0] == "TIMEOUT")
    
    print("\n" + "="*40)
    print("RETEST RESULTS")
    print("="*40)
    print(f"Total tested: {total}")
    print(f"Pass:    {pass_count} ({pass_count/total*100:.1f}%)")
    print(f"Fail:    {fail_count} ({fail_count/total*100:.1f}%)")
    print(f"Timeout: {timeout_count} ({timeout_count/total*100:.1f}%)")
    
    print("\nTOP REMAINING PATTERNS:")
    fail_details = [r[1] for r in results if r[0] == "FAIL"]
    
    def normalize_error(e):
        if not e: return "Unknown error"
        if "6.1.13" in e: return "6.1.13: Content stream contains real value exceeding 32767"
        if "6.2.11.8" in e: return "6.2.11.8: Content stream contains reference to .notdef glyph"
        if "6.9" in e: return "6.9: File specification has no /EF key"
        if "6.1.4" in e: return "6.1.4: Cross-reference streams (/Type /XRef) shall not be used"
        if "6.1.7.1" in e: return "6.1.7.1: Stream dictionary contains /F file specification"
        if "6.1.6" in e: return "6.1.6: Hexadecimal string contains non-hex characters"
        if "6.2.11.7" in e: return "6.2.11.7: ToUnicode CMap missing or forbidden mappings"
        return e[:100]

    norm_fails = [normalize_error(e) for e in fail_details]
    counts = Counter(norm_fails).most_common(10)
    for err, count in counts:
        print(f"{count:3d} x {err}")

    # Estimate based on 11,160 total failures
    total_failures_before = 11160
    failure_rate_among_failing = fail_count / total
    estimated_remaining = total_failures_before * failure_rate_among_failing
    
    print(f"\nESTIMATED TOTAL REMAINING FAILURES (on 342K): {int(estimated_remaining)}")
    print(f"(Based on {failure_rate_among_failing*100:.1f}% recurrence rate among previously failing files)")

if __name__ == "__main__":
    main()
