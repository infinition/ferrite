#!/usr/bin/env python3
"""Checks that no user facing string escapes the i18n system.

It compares the language catalogues against each other, verifies that every
rule declared in `src/catalog.rs` has a description in each language, and that
every key referenced by the template or the front end script exists everywhere.

Exits non zero when something is missing, so it can gate a build.

Usage: python tools/check_i18n.py
"""

import io
import json
import os
import re
import sys

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Keys assembled at runtime on the client, invisible to static analysis.
IGNORE_STATUSES = ["all", "partial", "none", "tracked", "na"]
RISKS = ["safe", "check", "data"]

DYNAMIC_FAMILIES = (
    ["ignore." + status for status in IGNORE_STATUSES]
    + ["ignore.tip_" + status for status in IGNORE_STATUSES]
    + ["risk." + risk for risk in RISKS]
    + ["risk." + risk + "_desc" for risk in RISKS]
)

RULE_MACROS = r"\b(?:d|f|fg|dreq|dunder)!\("


def flatten(data, prefix=""):
    out = {}
    for key, value in data.items():
        path = prefix + "." + key if prefix else key
        if isinstance(value, dict):
            out.update(flatten(value, path))
        else:
            out[path] = value
    return out


def read(*parts):
    with io.open(os.path.join(BASE, *parts), encoding="utf-8") as handle:
        return handle.read()


def load_catalogs():
    directory = os.path.join(BASE, "assets", "locales")
    catalogs = {}
    for name in sorted(os.listdir(directory)):
        if name.endswith(".json"):
            catalogs[name[:-5]] = flatten(json.loads(read("assets", "locales", name)))
    return catalogs


def rust_rules():
    """Rule identifiers and categories declared in the Rust catalogue.

    A rule is written either as a literal `Rule { id: "x", cat: "y", ... }` or
    through the `d!`, `f!`, `fg!`, `dreq!` and `dunder!` macros, whose first two
    arguments are always the identifier and the category.
    """
    source = read("src", "catalog.rs")
    body = source.split("pub static RULES", 1)[1].split("pub static PROJECT_MARKERS", 1)[0]

    ids = set(re.findall(r'id:\s*"([a-z0-9_]+)"', body))
    ids |= set(re.findall(RULE_MACROS + r'\s*"([a-z0-9_]+)"', body))

    categories = set(re.findall(r'cat:\s*"([a-z]+)"', body))
    categories |= set(re.findall(RULE_MACROS + r'\s*"[a-z0-9_]+",\s*"([a-z]+)"', body))

    return sorted(ids), sorted(categories)


def interface_keys():
    html = read("assets", "index.html")
    keys = re.findall(r'data-i18n="([^"]+)"', html)
    keys += [pair.split(":", 1)[1] for pair in re.findall(r'data-i18n-attr="([^"]+)"', html)]

    js = read("assets", "app.js")
    # Keys ending in an underscore are concatenated at runtime and are covered
    # by DYNAMIC_FAMILIES instead.
    keys += [k for k in re.findall(r"t\('([a-z_]+\.[a-z_]+)'", js) if not k.endswith("_")]
    return keys


def main():
    catalogs = load_catalogs()
    codes = sorted(catalogs)
    if not codes:
        print("no language catalogue found")
        return 1

    reference = set(catalogs[codes[0]])
    problems = []

    print("languages: %s" % ", ".join(codes))
    for code in codes:
        print("  %s: %d keys" % (code, len(catalogs[code])))
        for key in sorted(reference - set(catalogs[code])):
            problems.append("%s: missing key %s" % (code, key))
        for key in sorted(set(catalogs[code]) - reference):
            problems.append("%s: extra key %s" % (code, key))

    rule_ids, categories = rust_rules()
    for rule_id in rule_ids:
        for code in codes:
            if "rules.%s.desc" % rule_id not in catalogs[code]:
                problems.append("rule %s has no description in %s" % (rule_id, code))

    for category in categories:
        for code in codes:
            if "cat.%s" % category not in catalogs[code]:
                problems.append("category %s has no label in %s" % (category, code))

    keys = interface_keys()
    for key in sorted(set(keys)) + DYNAMIC_FAMILIES:
        for code in codes:
            if key not in catalogs[code]:
                problems.append("key %s missing in %s" % (key, code))

    print("keys referenced by the interface: %d" % len(set(keys)))
    print("rules in the Rust catalogue: %d" % len(rule_ids))
    print("categories: %d" % len(categories))

    if problems:
        print("\nPROBLEMS:")
        for problem in sorted(set(problems)):
            print("  - " + problem)
        return 1

    print("\nOK: no string outside i18n, every catalogue is aligned.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
