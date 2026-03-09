#!/usr/bin/env python3
"""Create tasks from error clusters for worker terminals to pick up."""
import json
import os
import sys
from datetime import datetime

ORCH_DIR = "/tmp/xfa-orchestrator"
TASKS_DIR = f"{ORCH_DIR}/tasks"

# Rule → likely fix domain mapping
RULE_DOMAIN = {
    "6.2.11.4.1": "font-embedding",
    "6.2.11.4.2": "font-embedding",
    "6.2.11.5": "font-widths",
    "6.2.11.2": "font-encoding",
    "6.2.11.3": "font-encoding",
    "6.2.11.6": "font-metrics",
    "6.2.11.8": "font-metrics",
    "6.2.4.3": "colorspace-cmyk",
    "6.2.4.2": "colorspace-rgb",
    "6.2.4.4": "colorspace-other",
    "6.2.9": "colorspace-rendering-intent",
    "6.3.3": "annotation-contents",
    "6.3.2": "annotation-flags",
    "6.3.1": "annotation-other",
    "6.9": "optional-content",
    "6.1.2": "file-header",
    "6.1.7": "file-structure",
    "6.1.13": "file-structure",
    "6.6.2.3.1": "metadata-xmp",
    "6.8": "transparency",
    "6.5.1": "interactive-forms",
    "6.4.1": "actions",
    "6.4.2": "actions",
    "6.2.2": "graphics-state",
    "6.2.5": "halftone",
    "6.2.6": "transfer-function",
}

# Domain → affected files mapping
DOMAIN_FILES = {
    "font-embedding": ["crates/pdf-manip/src/pdfa_fonts.rs"],
    "font-widths": ["crates/pdf-manip/src/pdfa_fonts.rs"],
    "font-encoding": ["crates/pdf-manip/src/pdfa_fonts.rs"],
    "font-metrics": ["crates/pdf-manip/src/pdfa_fonts.rs"],
    "colorspace-cmyk": ["crates/pdf-manip/src/pdfa_colorspace.rs"],
    "colorspace-rgb": ["crates/pdf-manip/src/pdfa_colorspace.rs"],
    "colorspace-other": ["crates/pdf-manip/src/pdfa_colorspace.rs"],
    "colorspace-rendering-intent": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "annotation-contents": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "annotation-flags": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "annotation-other": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "optional-content": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "file-header": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "file-structure": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "metadata-xmp": ["crates/pdf-manip/src/pdfa_xmp.rs"],
    "transparency": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "interactive-forms": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "actions": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "graphics-state": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "halftone": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
    "transfer-function": ["crates/pdf-manip/src/pdfa_cleanup.rs"],
}

# Priority descriptions for rules
RULE_HINTS = {
    "6.2.11.4.1:1": "Font programs must be embedded. Check Subtype matches FontFile type (Type1→FontFile, TrueType→FontFile2, CFF→FontFile3).",
    "6.2.11.5:1": "Glyph widths in font dict must match embedded font program. After embedding, update Widths array from actual font metrics.",
    "6.2.4.3:3": "DeviceCMYK needs CMYK OutputIntent or DefaultCMYK ICCBased colorspace.",
    "6.3.3:1": "Annotations must have /Contents key (even if empty string).",
    "6.3.3:2": "Annotations with /Contents must have it as a text string.",
    "6.9:1": "Optional content groups must be listed in /OCGs array of OCProperties.",
    "6.9:3": "Default config /D must list all OCGs in /Order or /AS.",
    "6.1.2:1": "PDF header must be %PDF-1.x with binary comment containing 4+ high bytes.",
    "6.6.2.3.1:1": "XMP metadata must contain dc:title.",
    "6.6.2.3.1:2": "XMP metadata properties must use correct value types.",
    "6.8:5": "Soft mask images must use DeviceGray.",
    "6.2.11.2:4": "CMap must be Identity-H/V or embedded.",
    "6.2.11.6:3": "Font metrics (Ascent/Descent/CapHeight) must be consistent.",
    "6.2.11.8:1": "CIDSet must cover all glyphs used in the font.",
    "6.5.1:1": "Form fields must have appearance streams.",
    "6.2.9:1": "Rendering intent must be RelativeColorimetric, AbsoluteColorimetric, Perceptual, or Saturation.",
}


def get_domain(rule_clause):
    """Map a rule clause to a fix domain."""
    for prefix, domain in sorted(RULE_DOMAIN.items(), key=lambda x: -len(x[0])):
        if rule_clause.startswith(prefix):
            return domain
    return "unknown"


def create_tasks(iteration_dir, iteration_num):
    """Read clusters and create tasks."""
    clusters_file = os.path.join(iteration_dir, "clusters.json")
    rules_file = os.path.join(iteration_dir, "rules.json")

    if not os.path.exists(clusters_file):
        print(f"No clusters file at {clusters_file}")
        return

    clusters = json.load(open(clusters_file))
    rules = json.load(open(rules_file)) if os.path.exists(rules_file) else []

    # Build rule description lookup
    rule_descs = {}
    for r in rules:
        key = f"{r['clause']}:{r['test_num']}"
        rule_descs[key] = r.get("description", "")

    # Clear old tasks for this iteration
    for f in os.listdir(TASKS_DIR):
        if f.startswith(f"iter{iteration_num:03d}-"):
            os.remove(os.path.join(TASKS_DIR, f))

    # Group clusters by primary domain
    domain_groups = {}
    for cluster in clusters:
        profile = cluster["profile"]
        rules_in_cluster = profile.split("|")
        pdf_count = cluster["pdf_count"]

        # Determine primary domain from the first rule
        primary_rule = rules_in_cluster[0].split(":")[0]
        domain = get_domain(primary_rule)

        if domain not in domain_groups:
            domain_groups[domain] = {
                "rules": set(),
                "pdf_count": 0,
                "profiles": [],
            }
        for r in rules_in_cluster:
            domain_groups[domain]["rules"].add(r)
        domain_groups[domain]["pdf_count"] += pdf_count
        domain_groups[domain]["profiles"].append(profile)

    # Create one task per domain (sorted by impact)
    task_num = 0
    for domain, info in sorted(
        domain_groups.items(), key=lambda x: -x[1]["pdf_count"]
    ):
        task_num += 1
        rules_list = sorted(info["rules"])
        hints = []
        for r in rules_list:
            if r in RULE_HINTS:
                hints.append(f"- {r}: {RULE_HINTS[r]}")
            elif r in rule_descs:
                hints.append(f"- {r}: {rule_descs[r]}")

        affected_files = DOMAIN_FILES.get(domain, [])

        task = {
            "id": f"iter{iteration_num:03d}-{task_num:03d}",
            "iteration": iteration_num,
            "domain": domain,
            "description": f"Fix {domain}: {info['pdf_count']} PDFs affected by {len(rules_list)} rules",
            "verapdf_rules": rules_list,
            "pdf_count": info["pdf_count"],
            "profiles": info["profiles"][:5],  # Top 5 profiles
            "affected_files": affected_files,
            "hints": hints,
            "status": "open",
            "assigned_to": None,
            "branch": None,
            "created_at": datetime.now().isoformat(),
            "completed_at": None,
            "result": None,
        }

        task_file = os.path.join(TASKS_DIR, f"{task['id']}.json")
        json.dump(task, open(task_file, "w"), indent=2)
        print(
            f"  Task {task['id']}: {domain} ({info['pdf_count']} PDFs, {len(rules_list)} rules)"
        )

    print(f"\nCreated {task_num} tasks in {TASKS_DIR}/")
    return task_num


if __name__ == "__main__":
    if len(sys.argv) < 2:
        # Find latest iteration
        iters = [
            d
            for d in os.listdir(f"{ORCH_DIR}/iterations")
            if d.startswith("iter-")
        ]
        if not iters:
            print("No iterations found")
            sys.exit(1)
        latest = sorted(iters)[-1]
        iter_num = int(latest.replace("iter-", ""))
    else:
        iter_num = int(sys.argv[1])

    iter_dir = f"{ORCH_DIR}/iterations/iter-{iter_num:03d}"
    print(f"=== Creating tasks for iteration {iter_num} ===")
    create_tasks(iter_dir, iter_num)
