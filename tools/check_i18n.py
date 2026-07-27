#!/usr/bin/env python3
"""Verifie qu'aucune chaine visible n'echappe au systeme i18n.

Compare les catalogues de langue entre eux, controle que chaque regle declaree
dans `src/catalog.rs` possede une description dans chaque langue, et que toutes
les cles referencees par le template et le script front existent partout.

Usage: python tools/check_i18n.py
"""

import io
import json
import os
import re
import sys

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Cles assemblees a l'execution cote client, invisibles pour l'analyse statique.
IGNORE_STATUSES = ["all", "partial", "none", "tracked", "na"]
RISKS = ["safe", "check", "data"]

DYNAMIC_FAMILIES = (
    ["ignore." + s for s in IGNORE_STATUSES]
    + ["ignore.tip_" + s for s in IGNORE_STATUSES]
    + ["risk." + r for r in RISKS]
    + ["risk." + r + "_desc" for r in RISKS]
)


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
    """Identifiants et categories declares dans le catalogue Rust.

    Les regles s'ecrivent soit en litteral `Rule { id: "x", cat: "y", ... }`,
    soit via les macros `d!`, `f!`, `fg!`, `dreq!`, `dunder!`, dont les deux
    premiers arguments sont toujours l'identifiant et la categorie.
    """
    source = read("src", "catalog.rs")
    body = source.split("pub static RULES", 1)[1].split("pub static PROJECT_MARKERS", 1)[0]

    ids = set(re.findall(r'id:\s*"([a-z0-9_]+)"', body))
    ids |= set(re.findall(r'\b(?:d|f|fg|dreq|dunder)!\(\s*"([a-z0-9_]+)"', body))

    categories = set(re.findall(r'cat:\s*"([a-z]+)"', body))
    categories |= set(re.findall(
        r'\b(?:d|f|fg|dreq|dunder)!\(\s*"[a-z0-9_]+",\s*"([a-z]+)"', body))

    return sorted(ids), sorted(categories)


def main():
    catalogs = load_catalogs()
    codes = sorted(catalogs)
    if not codes:
        print("aucun catalogue de langue trouve")
        return 1

    reference = set(catalogs[codes[0]])
    problems = []

    print("langues: %s" % ", ".join(codes))
    for code in codes:
        print("  %s: %d cles" % (code, len(catalogs[code])))
        for key in sorted(reference - set(catalogs[code])):
            problems.append("%s: cle manquante %s" % (code, key))
        for key in sorted(set(catalogs[code]) - reference):
            problems.append("%s: cle en trop %s" % (code, key))

    rule_ids, categories = rust_rules()
    for rule_id in rule_ids:
        for code in codes:
            if "rules.%s.desc" % rule_id not in catalogs[code]:
                problems.append("regle %s sans description en %s" % (rule_id, code))

    for category in categories:
        for code in codes:
            if "cat.%s" % category not in catalogs[code]:
                problems.append("categorie %s sans libelle en %s" % (category, code))

    html = read("assets", "index.html")
    keys = re.findall(r'data-i18n="([^"]+)"', html)
    keys += [m.split(":", 1)[1] for m in re.findall(r'data-i18n-attr="([^"]+)"', html)]

    js = read("assets", "app.js")
    keys += [k for k in re.findall(r"t\('([a-z_]+\.[a-z_]+)'", js) if not k.endswith("_")]

    for key in sorted(set(keys)) + DYNAMIC_FAMILIES:
        for code in codes:
            if key not in catalogs[code]:
                problems.append("cle %s absente en %s" % (key, code))

    print("cles referencees dans l'interface: %d" % len(set(keys)))
    print("regles du catalogue Rust: %d" % len(rule_ids))
    print("categories: %d" % len(categories))

    if problems:
        print("\nPROBLEMES:")
        for problem in sorted(set(problems)):
            print("  - " + problem)
        return 1

    print("\nOK: aucune chaine hors i18n, tous les catalogues sont alignes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
