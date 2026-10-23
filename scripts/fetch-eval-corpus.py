#!/usr/bin/env python3
"""Fetch the eval corpus: English Wikipedia articles converted to clean markdown.

Writes eval/corpus/<slug>.md (frontmatter: title, source, revision, license),
deleting any previous eval/corpus/*.md first. Dev-time only; the committed
files, not a re-run, are the reference (a re-fetch changes the text).

Usage (from the repo root):
    uv run --with markdownify --with beautifulsoup4 scripts/fetch-eval-corpus.py
"""
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

from bs4 import BeautifulSoup, NavigableString
from markdownify import markdownify

ARTICLES = [
    "Clinical trial", "Phases of clinical research", "Randomized controlled trial",
    "Blinded experiment", "Placebo", "Placebo-controlled study", "Informed consent",
    "Institutional review board", "Good clinical practice", "Declaration of Helsinki",
    "Drug development", "Drug discovery", "Preclinical development", "Pharmacokinetics",
    "Pharmacodynamics", "Bioavailability", "Biological half-life", "Therapeutic index",
    "Dose–response relationship", "Adverse event", "Pharmacovigilance", "Clinical endpoint",
    "Surrogate endpoint", "Survival analysis", "Kaplan–Meier estimator", "Hazard ratio",
    "Meta-analysis", "Systematic review", "Adaptive design (medicine)",
    "Real-world evidence", "Contract research organization", "Food and Drug Administration",
    "European Medicines Agency", "New Drug Application", "Investigational New Drug",
    "Orphan drug", "Generic drug", "Biosimilar", "Monoclonal antibody",
    "Clinical data management", "Clinical Data Interchange Standards Consortium",
    "Electronic data capture", "Intention-to-treat analysis", "Power (statistics)",
    "Thalidomide",
]

USER_AGENT = "vlt-eval-corpus/0.1 (https://github.com/andresdigiovanni/vlt)"
OUT_DIR = Path("eval/corpus")
DROP_SELECTORS = [
    "style", "script", "link", "meta", "figure", "img", "sup.reference", ".mw-ref",
    ".reference", ".infobox", ".navbox", ".vertical-navbox", ".sidebar", ".hatnote",
    ".ambox", ".metadata", ".noprint", ".mw-editsection", ".reflist",
    ".mw-references-wrap", ".shortdescription", ".thumb", ".gallery",
    # beyond the base list, found while inspecting output
    "figcaption", "[typeof~='mw:Transclusion'] .navbar", ".mw-empty-elt", ".portal",
    ".side-box", ".box-More_citations_needed", ".cs1-maint", "[role='navigation']",
    ".citation-needed-content",
]
DROP_SECTIONS = {
    "see also", "notes", "references", "further reading", "external links",
    "bibliography", "citations", "sources", "footnotes",
}


def is_dropped_section(heading):
    h = heading.strip().lower()
    return h in DROP_SECTIONS or h == "resources" or "external links" in h


def get(url):
    time.sleep(0.5)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req) as r:
        if r.status != 200:
            sys.exit(f"HTTP {r.status} for {url}")
        return r.read().decode("utf-8")


def resolve(title):
    q = urllib.parse.urlencode({
        "action": "query", "prop": "revisions", "rvprop": "ids", "titles": title,
        "redirects": 1, "format": "json",
    })
    data = json.loads(get(f"https://en.wikipedia.org/w/api.php?{q}"))
    page = next(iter(data["query"]["pages"].values()))
    if "missing" in page or "revisions" not in page:
        sys.exit(f"title does not resolve: {title}")
    return page["title"], page["revisions"][0]["revid"]


def slugify(title):
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")


def math_text(el):
    m = el.find("math")
    alt = (m.get("alttext") if m else None) or el.get_text()
    alt = re.sub(r"\s+", " ", alt).strip()
    mm = re.fullmatch(r"\{\\(?:displaystyle|textstyle)\s*(.*)\}", alt)
    return mm.group(1).strip() if mm else alt


def clean_html(html, maths):
    soup = BeautifulSoup(html, "html.parser")
    for el in soup.select(".mwe-math-element"):
        text = math_text(el)
        if el.find_parent(["td", "th"]):
            text = text.replace("|", "\\|")
        # placeholder: markdownify would escape `_` and the tag stripper would eat `<`
        maths.append(text)
        el.replace_with(soup.new_string(f"VLTMATH{len(maths) - 1}X"))
    # leaked wikitext: a bibliography entry typed as plain text right after a ref marker
    # (it runs to the end of its source line), and a `* Title, date.` footnote left inline
    for sup in soup.select("sup.mw-ref"):
        nxt = sup.next_sibling
        if isinstance(nxt, NavigableString):
            nxt.replace_with(re.sub(r"^\s*[A-Z][\w'-]+ [A-Z]{1,2}\. [^\n]*: [^\n]*(\n|$)", " ", str(nxt)))
    for node in soup.find_all(string=re.compile(r"[.?!]\*\s")):
        if node.parent is None:
            continue
        i = re.search(r"[.?!]\*\s", node).start() + 1
        stop = []
        for sib in node.next_siblings:
            if getattr(sib, "name", None) == "sup" and "mw-ref" in (sib.get("class") or []):
                break
            stop.append(sib)
        for sib in stop:
            sib.extract()
        node.replace_with(str(node)[:i])
    # Webarchive templates: "Archived <date> at the Wayback Machine" is link furniture, not prose
    for el in soup.find_all(attrs={"data-mw": re.compile(r'"wt":"Webarchive"')}):
        about = el.get("about")
        for part in (soup.find_all(attrs={"about": about}) if about else [el]):
            part.decompose()
    for sel in DROP_SELECTORS:
        for el in soup.select(sel):
            el.decompose()
    for sec in soup.find_all("section"):
        h = sec.find(re.compile(r"^h[1-6]$"))
        if h and is_dropped_section(h.get_text()) and sec.parent is not None:
            sec.decompose()
    for a in soup.find_all("a"):
        a.unwrap()
    # HTML line breaks inside prose are whitespace: keep a paragraph on one line
    for node in soup.find_all(string=re.compile(r"\n")):
        if node.find_parent(["pre", "code"]) is None:
            node.replace_with(re.sub(r"[ \t]*\n\s*", " ", str(node)))
    # definition lists are mostly indented display math / sub-paragraphs: make them paragraphs
    for dl in soup.find_all("dl"):
        for item in dl.find_all(["dt", "dd"]):
            item.name = "p"
        dl.unwrap()
    # tables left empty once their images and navigation were dropped
    for table in soup.find_all("table"):
        if not table.get_text().strip():
            table.decompose()
    # tables: header-less tables get their first row promoted so they convert
    for table in soup.find_all("table"):
        for el in table.find_all(["caption"]):
            el.decompose()
        for el in table.find_all(["br"]):
            el.replace_with(" ")
        for el in table.find_all(["ul", "ol", "p", "div"]):
            el.unwrap()
        for li in table.find_all("li"):
            li.unwrap()
    return soup


def tidy(md):
    md = re.sub(r"\[\d+\]", "", md)
    md = re.sub(r"\[(?:citation needed|clarification needed|when\?|who\?|according to whom\?)\]", "", md, flags=re.I)
    md = re.sub(r"\[edit\]", "", md)
    md = re.sub(r"(?<=[A-Za-z]) +\.(?= |$)", ".", md)
    md = re.sub(r"</?(div|span)[^>]*>", "", md)
    md = re.sub(r"<[^>\n]+>", "", md)
    lines = [l.rstrip() for l in md.split("\n")]
    # drop empty table rows (every cell blank)
    lines = [l for l in lines if not re.fullmatch(r"\|(\s*\|)+", l.strip()) or re.fullmatch(r"\|(\s*-+\s*\|)+", l.strip())]
    md = "\n".join(lines)
    md = re.sub(r"\n{3,}", "\n\n", md)
    # drop headings with no content before the next same-or-higher heading / EOF
    md = re.sub(r"[\u200b\u200c\u200d\ufeff]", "", md).replace("\xa0", " ")
    changed = True
    while changed:
        changed = False
        ls = md.split("\n")
        for i, l in enumerate(ls):
            m = re.match(r"(#{2,6}) ", l)
            if not m:
                continue
            j = i + 1
            while j < len(ls) and not ls[j].strip():
                j += 1
            n = re.match(r"(#{1,6}) ", ls[j]) if j < len(ls) else None
            if j >= len(ls) or (n and len(n.group(1)) <= len(m.group(1))):
                del ls[i:j]
                md = "\n".join(ls)
                changed = True
                break
    return md.strip() + "\n"


def yaml_str(s):
    if re.search(r"[:\"'#]", s) or re.match(r"[-\[{&*!|>%@`]", s):
        return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return s


def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for old in OUT_DIR.glob("*.md"):
        old.unlink()
    for title in ARTICLES:
        canon, revid = resolve(title)
        path = urllib.parse.quote(canon.replace(" ", "_"), safe="")
        html = get(f"https://en.wikipedia.org/api/rest_v1/page/html/{path}/{revid}")
        maths = []
        soup = clean_html(html, maths)
        body = soup.body or soup
        md = markdownify(str(body), heading_style="ATX", bullets="-", strip=["a"])
        md = tidy(md)
        md = re.sub(r"VLTMATH(\d+)X", lambda m: f"`{maths[int(m.group(1))]}`", md)
        source = "https://en.wikipedia.org/wiki/" + urllib.parse.quote(canon.replace(" ", "_"), safe="_()-,.'")
        out = (f"---\ntitle: {yaml_str(canon)}\nsource: {source}\nrevision: {revid}\n"
               f"license: CC BY-SA 4.0\n---\n\n# {canon}\n\n{md}")
        (OUT_DIR / f"{slugify(canon)}.md").write_text(out, encoding="utf-8")
        print(f"{canon}: {len(out)} bytes", file=sys.stderr)


if __name__ == "__main__":
    main()
