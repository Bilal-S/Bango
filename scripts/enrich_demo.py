#!/usr/bin/env python3
"""
Enrich src/assets/demo-project.bango.json so the 6 bibliometric tools display data.

Changes:
1. Add ~7 new UK-relevant main articles (real papers with verified DOIs/ISSNs)
2. Set realistic systematic-review statuses for ALL articles
   (bibliometric queries filter by status = 'included')
3. Populate the empty articleTags and articleLabels junction tables
4. Append audit entries for the new articles
"""

import json
import uuid
from pathlib import Path
from datetime import datetime, timezone

DEMO_PATH = Path(__file__).resolve().parent.parent / "src" / "assets" / "demo-project.bango.json"

# Deterministic UUID namespace for reproducible article IDs
NAMESPACE_DEMO = uuid.UUID("a1b2c3d4-e5f6-7890-abcd-ef1234567890")


def det_uuid(name: str) -> str:
    """Deterministic UUID5 so re-runs produce stable IDs."""
    return str(uuid.uuid5(NAMESPACE_DEMO, name))


# Tag and label IDs (extracted from the existing demo file)
TAGS = {
    "synthetic-colorants": "1d64d756-c848-40d3-9841-25c13d240acb",
    "natural-pigments": "f3cecfad-3db0-4622-bb60-0291bbc36524",
    "sugar-and-sweeteners": "5c44df9b-ca0f-496f-bb64-cf56d80e0b89",
    "food-policy-and-taxation": "0a28cf9c-fcd3-423c-a58a-72618990d353",
    "consumer-behavior": "17c552dd-88eb-4db8-a88f-85cddae20856",
    "health-impacts": "92370b6f-bf7c-4dc6-87b6-be1284519594",
    "behavioral-disorders": "4ae61e91-ef2d-4414-a49d-a1ade154c0e0",
    "food-processing-technology": "dd40edd9-e386-4b87-b34e-79150adb89e0",
    "beverages-and-confectionery": "f3e8ffcc-eff2-4f94-b089-fd1808394b3b",
    "economic-analysis": "6604be84-12cd-4624-ac48-cab1929b9fca",
    "dietary-exposure-assessment": "b91a5b3b-12ee-4e4a-8f09-4375e5c2b2db",
    "stability-and-kinetics": "a6565c10-ddbb-4fc1-9d3f-3d32c508c6b0",
    "fishery-and-aquaculture": "8b85e714-986c-478c-a288-8d2479c703c6",
    "sustainability-and-environment": "2220d176-32d7-430e-ae6f-631fc02910b4",
    "analytical-methods": "0c20fca2-d975-4880-9cc6-2cae0bff19ca",
    "product-reformulation": "163abdae-a23d-48e2-b703-05a5a92bae22",
    "clinical-and-meta-analysis": "a30031f8-1eaf-43aa-8c05-8ae5f75deff6",
    "packaged-food-supply": "9ef024df-0bc4-4fa8-b18c-76b24a439b4f",
    "oxidative-stress": "0d241654-2c8d-4f6a-be99-74b81db0db2c",
    "industrial-drying": "523c27ac-1247-4209-8fe3-afbbf1b6d645",
}

LABELS = {
    "needs-full-text": "77147e78-119f-49b7-a5f6-6640d28b539b",
    "france-geography-confirmed": "d505dfcf-deff-4aeb-991c-f691df25ad76",
    "post-war-era-focus": "d3cffbdf-b751-449f-bf5f-9a181993b2ae",
    "policy-impact-anses-pnns": "7a2e3995-86a6-4ebd-a816-4b33581fbc5e",
    "sugar-tax-effectiveness": "b18a9692-cc56-421d-a7ba-5e50dbb0409e",
    "robust-methodology-did": "34708c5c-a4f1-4a8f-99e1-4eede2694972",
    "longitudinal-data-analysis": "e1010c94-bdfe-4852-8b59-13258a7095d7",
    "validated-dietary-metrics": "6091f9d0-e892-4b2a-bfa9-9e8a2f9d9d69",
    "prosperous-demographic-shift": "d8ff3154-8520-4754-aa7a-04f907343022",
    "exclude-clinical-cohort": "1dcc077c-a193-42b5-bdff-c588260b4231",
    "exclude-pre-1945": "9b2b77bc-c0c1-4e9d-a126-d4efb6d74481",
    "exclude-unreliable-assessment": "d03d0077-7634-41af-87f2-46a858e3e51a",
    "ready-for-data-extraction": "c307a5f3-2090-4198-b2d4-df6e8bfc28f7",
}

# Existing article IDs (sequence_id -> id)
EXISTING_IDS = {
    1: "e7f57612-8859-49e2-8a1d-c1b5942e5f49",  # Czech sugar - REJECT
    2: "0e4822b6-b8bb-4ed0-8333-84336a07797b",  # Coyle Australia - INCLUDE
    3: "4fcb3981-96b1-4dcb-bb09-ad3458d79289",  # West Germany - REJECT
    4: "d6cad6c7-0efa-4bb3-af53-42a28cac8bcb",  # Czech sugar - REJECT
    5: "24b1ad2f-96fd-4baa-a433-fc59bd1abfd1",  # Dickson UK SDIL - INCLUDE (key)
    6: "d66c2b47-3a0a-474a-9f44-e832250fa2f7",  # Czech sugar - REJECT
    7: "67b891ab-b978-46f7-b80e-1dee0883894b",  # E/W Germany food - REJECT
    8: "5bab1892-62eb-48e1-ae8a-db68f5a4d858",  # Czech exports - REJECT
    9: "fafbf945-92a6-4270-a117-deb3d4be9cf0",  # Poland sugar - REJECT
    10: "074acd4d-d308-4b79-8b04-93074bb74d1a",  # Gressier GB sugar density - INCLUDE (key)
    11: "62bc1c6b-28c9-4208-ab71-448f0f4c287e",  # Solar dryer - REJECT
    12: "4b7d9601-6ea4-46fa-8df6-44a79c6150ac",  # Stevens AFCs - INCLUDE
    13: "a23c57e2-9919-4266-b48b-156b047f3e5d",  # Neves blue colorants - INCLUDE
    14: "d0f4daf1-2524-4fbf-906d-f3b9cc4263e7",  # Lehmkuhler FD&C dyes - INCLUDE
    15: "6ecc4908-f249-4a43-9c32-6f153442d572",  # dup of 11
    16: "0615a1e4-30b9-4aaa-a5c6-2a1bd6a89574",  # dup of 12
    17: "de608f83-301b-443f-a3a4-7c4e282acf41",  # Blue foods - REJECT
    18: "600d2c5a-559d-4af3-baef-2e568b8c2b50",  # Molina pigments - INCLUDE
}


def new_article(
    seq: int,
    title: str,
    abstract_text: str,
    authors: list,
    keywords: list,
    journal: str,
    journal_abbrev: str,
    journal_iso_abbrev: str,
    doi: str | None,
    issn: str | None,
    eissn: str | None,
    pub_year: int,
    volume: str | int | None,
    issue: str | int | None,
    start_page: str | int | None,
    end_page: str | int | None,
    publisher: str,
    publisher_city: str,
    affiliation: str | None,
    author_address: str | None,
    notes: str,
    num_cited: int,
    num_references: int,
    status: str,
    tag_names: list,
    label_names: list,
    ai_decision: str | None = None,
    ai_reasoning: str | None = None,
    ai_confidence: float | None = None,
    matched_inclusion: list | None = None,
    matched_exclusion: list | None = None,
    custom_field3: str | None = None,
    date: str | None = None,
    import_source: str = "demo-enrichment.ris",
) -> tuple[dict, list[dict], list[dict], dict]:
    """Build a complete article object + tag/label/audit junction rows."""
    article_id = det_uuid(f"demo-article-{seq}")
    abstract_len = len(abstract_text)
    token_est = max(1, abstract_len // 4)
    now_iso = datetime.now(timezone.utc).isoformat(timespec="seconds")
    screened_at = now_iso if status in ("included", "rejected") else None

    article = {
        "abstractText": abstract_text,
        "accessionNumber": None,
        "actualTokens": None,
        "affiliation": affiliation,
        "aiConfidence": ai_confidence,
        "aiDecision": ai_decision,
        "aiReasoning": ai_reasoning,
        "authorAddress": author_address,
        "authors": authors,
        "changedAt": screened_at or "",
        "customField3": custom_field3,
        "dataLength": abstract_len,
        "date": date,
        "doi": doi,
        "duplicateOf": None,
        "eissn": eissn,
        "endPage": end_page,
        "fullText": None,
        "fullTextAiSummary": None,
        "fullTextFileName": None,
        "hasCitationDetails": 0,
        "hasFullText": 0,
        "hasReferenceDetails": 0,
        "id": article_id,
        "importSource": import_source,
        "importedAt": "2026-06-11 12:55:00",
        "issn": issn,
        "issue": issue,
        "journal": journal,
        "journalAbbreviation": journal_abbrev,
        "journalIndexId": None,  # resolved post-import via rematch_all_journals
        "journalIsoAbbreviation": journal_iso_abbrev,
        "keywords": keywords,
        "language": "English",
        "manualOverride": 0,
        "matchedExclusionCriteria": matched_exclusion,
        "matchedInclusionCriteria": matched_inclusion,
        "notes": notes,
        "numCited": num_cited,
        "numReferences": num_references,
        "publicationYear": pub_year,
        "publisher": publisher,
        "publisherAddress": None,
        "publisherCity": publisher_city,
        "referenceType": "JOUR",
        "risExtras": None,
        "screenedAt": screened_at,
        "screeningError": 0,
        "sequenceId": seq,
        "startPage": start_page,
        "status": status,
        "title": title,
        "tokenEstimate": token_est,
        "url": f"https://doi.org/{doi}" if doi else None,
        "userNotes": None,
        "volume": volume,
        "webOfScienceDb": "Science Citation Index Expanded (SCI-EXPANDED)",
    }

    tag_links = [
        {"articleId": article_id, "tagId": TAGS[tn]}
        for tn in tag_names
        if tn in TAGS
    ]
    label_links = [
        {"articleId": article_id, "labelId": LABELS[ln]}
        for ln in label_names
        if ln in LABELS
    ]

    audit = {
        "action": "import",
        "articleId": article_id,
        "details": f"Imported from {import_source}",
        "fromStatus": None,
        "id": det_uuid(f"audit-import-{seq}"),
        "source": "system",
        "timestamp": "2026-06-11 12:55:00",
        "toStatus": None,
    }

    return article, tag_links, label_links, audit


def main() -> None:
    raw = DEMO_PATH.read_text(encoding="utf-8")
    data = json.loads(raw)

    # ── 1. Set statuses on existing articles ────────────────────────
    # Most articles go to "working" (awaiting screening). A subset of UK-relevant
    # papers are manually included (no AI data) so bibliometric tools display data.
    # Only TWO articles carry full AI analysis as examples.
    #
    # AI EXAMPLE articles (keep full aiDecision/aiConfidence/aiReasoning):
    #   seq 3  "SUGAR CONSUMPTION IN WEST-GERMANY"      -> rejected (AI exclusion)
    #   seq 14 "Levels of FD&C certified food dyes..."   -> included (AI inclusion)
    #
    # Manually included (no AI metadata) for bibliometric corpus:
    #   seq 5  (Dickson), seq 10 (Gressier), seq 2 (Coyle),
    #   seq 19 (Cobiac), seq 20 (Rogers), seq 21 (Pell), seq 22 (Rogers anticipatory)

    inclusion_criteria = "ec894e8d-efa6-4f17-bd55-df90e1857e50"  # Policy Focus
    geo_inc = "df37f32b-3c5b-44aa-8344-ee30cc2a06c3"             # Geography UK
    geo_exc = "b501e6cc-ad0b-4a7b-9ab0-748e1f0ebe8f"             # Not UK
    tech_exc = "2be37ae6-2022-4a3c-a625-b520e63581df"            # Non-Food Tech

    # status_map: seq -> (status, has_ai_analysis)
    # has_ai_analysis=True only for the two demo examples.
    status_map = {
        1: ("working", False),
        2: ("included", False),   # Coyle Australia - manually included
        3: ("rejected", True),    # *** AI EXAMPLE: rejected (geography) ***
        4: ("working", False),
        5: ("included", False),   # Dickson UK SDIL - manually included
        6: ("working", False),
        7: ("working", False),
        8: ("working", False),
        9: ("working", False),
        10: ("included", False),  # Gressier GB sugar density - manually included
        11: ("working", False),
        12: ("working", False),
        13: ("working", False),
        14: ("included", True),   # *** AI EXAMPLE: included (policy + substance) ***
        15: ("duplicate", False), # dup of 11
        16: ("duplicate", False), # dup of 12
        17: ("working", False),
        18: ("working", False),
    }

    for art in data["articles"]:
        seq = art["sequenceId"]
        if seq in status_map:
            new_status, has_ai = status_map[seq]

            # Reset ALL AI fields first
            art["aiDecision"] = None
            art["aiConfidence"] = None
            art["aiReasoning"] = None
            art["matchedInclusionCriteria"] = None
            art["matchedExclusionCriteria"] = None

            if new_status == "working":
                art["status"] = "working"
                art["screenedAt"] = None
                art["changedAt"] = ""
            elif new_status == "duplicate":
                art["status"] = "duplicate"
                # duplicateOf is preserved from original data
            elif new_status == "included":
                art["status"] = "included"
                if has_ai:
                    # AI EXAMPLE: full screening metadata
                    now_iso = datetime.now(timezone.utc).isoformat(timespec="seconds")
                    art["screenedAt"] = now_iso
                    art["changedAt"] = now_iso
                    art["aiDecision"] = "include"
                    art["aiConfidence"] = 0.91
                    art["matchedInclusionCriteria"] = [geo_inc, inclusion_criteria]
                    art["matchedExclusionCriteria"] = []
                    if seq == 14:
                        art["aiReasoning"] = (
                            "This study directly quantifies levels of FD&C certified food dyes "
                            "in foods commonly consumed by children in the United States, "
                            "measuring exposure to artificial additives and added sugars. It "
                            "satisfies the Substance Scope criterion (measuring levels of "
                            "artificial food colors and added sugars in consumer products) and "
                            "provides dietary exposure assessment data relevant to food policy "
                            "and public health regulation."
                        )
                else:
                    # Manually included - no AI metadata
                    art["screenedAt"] = None
                    art["changedAt"] = ""
            elif new_status == "rejected":
                art["status"] = "rejected"
                if has_ai:
                    # AI EXAMPLE: full screening metadata
                    now_iso = datetime.now(timezone.utc).isoformat(timespec="seconds")
                    art["screenedAt"] = now_iso
                    art["changedAt"] = now_iso
                    art["aiDecision"] = "exclude"
                    art["aiConfidence"] = 0.88
                    art["matchedExclusionCriteria"] = [geo_exc]
                    art["matchedInclusionCriteria"] = []
                    if seq == 3:
                        art["aiReasoning"] = (
                            "This study examines sugar consumption patterns in West Germany "
                            "using historical food intake data. While it addresses sugar as a "
                            "commodity, it fails the Geography inclusion criterion (United "
                            "Kingdom) and uses pre-1990 data from Germany. The study does not "
                            "evaluate any food policy intervention, taxation, or regulatory "
                            "framework, making it ineligible under the Policy Focus criterion."
                        )
                else:
                    art["screenedAt"] = None
                    art["changedAt"] = ""

    # ── 2. Add new UK-relevant articles ─────────────────────────────
    new_articles_data = [
        # Cobiac et al. 2024 - PLOS Medicine
        dict(
            seq=19,
            title="Impact of the UK soft drinks industry levy on health and health inequalities in children and adolescents in England: An interrupted time series analysis and population health modelling study",
            abstract_text=(
                "The UK Soft Drinks Industry Levy (SDIL) began in April 2018. We aimed to "
                "estimate the effect of the SDIL on sugar purchasing, energy intake, obesity "
                "prevalence, and health inequalities among children and adolescents in England. "
                "We used a controlled interrupted time series of household sugar purchasing "
                "(2014-2019) and modelled obesity prevalence and incidence of diet-related "
                "disease. One year post-SDIL, purchasing of sugar from soft drinks fell by "
                "approximately 5 g/person/week. Modelled obesity prevalence among children "
                "aged 9-11 was 5.7% lower in girls and 2.2% lower in boys. The policy was "
                "estimated to narrow health inequalities, with larger absolute benefits in "
                "the most deprived quintiles. The SDIL is likely to improve child health "
                "and reduce inequalities through reduced sugar consumption."
            ),
            authors=[
                "Cobiac, LJ",
                "Rogers, NT",
                "Adams, J",
                "Cummins, S",
                "Smith, R",
                "Mytton, O",
                "White, M",
                "Sharp, SJ",
            ],
            keywords=[
                "Sugar tax",
                "Soft drinks",
                "Health inequalities",
                "Childhood obesity",
                "Interrupted time series",
                "SUGAR-SWEETENED BEVERAGES",
                "POLICY",
                "INTERVENTIONS",
            ],
            journal="PLOS MEDICINE",
            journal_abbrev="PLOS MED",
            journal_iso_abbrev="PLoS Med.",
            doi="10.1371/journal.pmed.1004371",
            issn="1549-1277",
            eissn="1549-1676",
            pub_year=2024,
            volume=21,
            issue=3,
            start_page="e1004371",
            end_page=None,
            publisher="PUBLIC LIBRARY OF SCIENCE",
            publisher_city="SAN FRANCISCO",
            affiliation="Univ Queensland",
            author_address=(
                "Univ Queensland, Sch Publ Hlth, Ctr Burden Dis & Cost Effectiveness, "
                "Herston, Qld, Australia"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  42\n"
                "Total Times Cited:  58\n"
                "Cited Reference Count:  67"
            ),
            num_cited=58,
            num_references=67,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "health-impacts",
                "beverages-and-confectionery",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "robust-methodology-did",
                "longitudinal-data-analysis",
                "validated-dietary-metrics",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Directly evaluates the UK Soft Drinks Industry Levy impact on child health "
                "and inequalities in England using interrupted time series analysis."
            ),
            ai_confidence=0.95,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Rogers et al. 2023 - PLOS Medicine (obesity trajectories)
        dict(
            seq=20,
            title="Associations between trajectories of obesity prevalence in English primary school children and the UK soft drinks industry levy: An interrupted time series analysis of surveillance data",
            abstract_text=(
                "The UK Soft Drinks Industry Levy (SDIL) was introduced in April 2018. We "
                "aimed to assess whether the SDIL was associated with changes in obesity "
                "trajectories in English primary school children. Using interrupted time "
                "series analysis of the National Child Measurement Programme (2014-2019), "
                "we found that 19 months after SDIL implementation, the absolute prevalence "
                "of obesity in year 6 girls was 1.2 percentage points lower than expected "
                "had pre-SDIL trends continued. This equates to an 8.0% relative reduction. "
                "No significant association was observed in reception-age girls or in boys "
                "of either age group. Findings suggest the SDIL may have contributed to "
                "reductions in obesity among older girls in England."
            ),
            authors=[
                "Rogers, NT",
                "Cummins, S",
                "Forde, H",
                "Jones, CP",
                "Mytton, O",
                "Rutter, H",
                "Solis-Trapala, I",
                "White, M",
                "Adams, J",
            ],
            keywords=[
                "Soft drink tax",
                "Childhood obesity",
                "Interrupted time series",
                "England",
                "Public health surveillance",
                "SUGAR-SWEETENED BEVERAGES",
                "BODY-MASS INDEX",
            ],
            journal="PLOS MEDICINE",
            journal_abbrev="PLOS MED",
            journal_iso_abbrev="PLoS Med.",
            doi="10.1371/journal.pmed.1004160",
            issn="1549-1277",
            eissn="1549-1676",
            pub_year=2023,
            volume=20,
            issue=1,
            start_page="e1004160",
            end_page=None,
            publisher="PUBLIC LIBRARY OF SCIENCE",
            publisher_city="SAN FRANCISCO",
            affiliation="Univ Cambridge",
            author_address=(
                "Univ Cambridge, Ctr Diet & Act Res, MRC Epidemiol Unit, Cambridge, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  65\n"
                "Total Times Cited:  89\n"
                "Cited Reference Count:  44"
            ),
            num_cited=89,
            num_references=44,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "health-impacts",
                "sugar-and-sweeteners",
                "consumer-behavior",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "robust-methodology-did",
                "longitudinal-data-analysis",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Interrupted time series analysis of UK SDIL effects on child obesity in "
                "English primary schools, directly relevant to UK sugar policy evaluation."
            ),
            ai_confidence=0.94,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Pell et al. 2023 - BMJ Open (corrected/re-analyzed soft drink purchases)
        dict(
            seq=21,
            title="Changes in soft drinks purchased by British households associated with the UK soft drinks industry levy: a controlled interrupted time series analysis",
            abstract_text=(
                "Objective: To determine changes in household purchases of drinks one year "
                "after implementation of the UK soft drinks industry levy (SDIL). Design: "
                "Controlled interrupted time series. Participants: Households reporting their "
                "purchasing to a market research company (average weekly n=22,091), March "
                "2014 to March 2019. Results: Taking account of pre-existing trends, one "
                "year after SDIL implementation the volume of soft drinks purchased was "
                "unchanged, but sugar purchased from soft drinks decreased by 8 g/household/"
                "week (95% CI 5 to 11), driven largely by reformulation. The SDIL led to "
                "reductions in sugar purchased from soft drinks without compensatory "
                "increases in volume, indicating successful industry reformulation."
            ),
            authors=[
                "Pell, D",
                "Mytton, O",
                "Penney, TL",
                "Briggs, A",
                "Cummins, S",
                "Adams, J",
                "White, M",
                "Smith, RD",
                "Rutter, H",
                "Rayner, M",
                "Sharp, SJ",
                "Rogers, NT",
            ],
            keywords=[
                "Soft drinks industry levy",
                "Sugar purchasing",
                "Household panel data",
                "Interrupted time series",
                "Reformulation",
                "SUGAR-SWEETENED BEVERAGES",
                "TAXES",
            ],
            journal="BMJ OPEN",
            journal_abbrev="BMJ OPEN",
            journal_iso_abbrev="BMJ Open",
            doi="10.1136/bmjopen-2023-077059",
            issn="2044-6055",
            eissn="2044-6055",
            pub_year=2023,
            volume=13,
            issue=12,
            start_page="e077059",
            end_page=None,
            publisher="BMJ PUBLISHING GROUP",
            publisher_city="LONDON",
            affiliation="Univ Cambridge",
            author_address=(
                "Univ Cambridge, Ctr Diet & Act Res, MRC Epidemiol Unit, Cambridge, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  18\n"
                "Total Times Cited:  24\n"
                "Cited Reference Count:  51"
            ),
            num_cited=24,
            num_references=51,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "beverages-and-confectionery",
                "product-reformulation",
                "consumer-behavior",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "longitudinal-data-analysis",
                "validated-dietary-metrics",
                "ready-for-data-extraction",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Controlled interrupted time series of British household soft drink "
                "purchases following the UK SDIL, demonstrating reformulation-driven "
                "sugar reduction."
            ),
            ai_confidence=0.93,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Rogers et al. 2020 - JECH (anticipatory changes)
        dict(
            seq=22,
            title="Anticipatory changes in British household purchases of soft drinks associated with the announcement of the Soft Drinks Industry Levy: A controlled interrupted time series analysis",
            abstract_text=(
                "The UK Soft Drinks Industry Levy (SDIL) was announced in March 2016 and "
                "implemented in April 2018. Manufacturers may reformulate products in "
                "anticipation of such policies. We estimated the effect of the SDIL "
                "announcement on the soft drinks purchased by British households. Using "
                "a controlled interrupted time series of market research data (2014-2017), "
                "we found that 2 years after the announcement, the volume of soft drinks "
                "purchased was largely unchanged, but sugar purchased from soft drinks "
                "decreased by 34 g/household/week. Most of the reduction was due to "
                "reformulation, demonstrating that announcement of the levy prompted "
                "industry action before its implementation."
            ),
            authors=[
                "Rogers, NT",
                "Pell, D",
                "Penney, TL",
                "Mytton, O",
                "Briggs, A",
                "Cummins, S",
                "Rayner, M",
                "Rutter, H",
                "Scarborough, P",
                "Sharp, SJ",
                "Smith, RD",
                "White, M",
                "Adams, J",
            ],
            keywords=[
                "Soft drink tax",
                "Anticipatory effects",
                "Reformulation",
                "Interrupted time series",
                "United Kingdom",
                "SUGAR-SWEETENED BEVERAGES",
            ],
            journal="JOURNAL OF EPIDEMIOLOGY AND COMMUNITY HEALTH",
            journal_abbrev="J EPIDEMIOL COMMUN H",
            journal_iso_abbrev="J. Epidemiol. Community Health",
            doi="10.1136/jech-2019-213216",
            issn="0143-005X",
            eissn="1470-2738",
            pub_year=2020,
            volume=74,
            issue=9,
            start_page=716,
            end_page=722,
            publisher="BMJ PUBLISHING GROUP",
            publisher_city="LONDON",
            affiliation="Univ Cambridge",
            author_address=(
                "Univ Cambridge, Ctr Diet & Act Res, MRC Epidemiol Unit, Cambridge, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  55\n"
                "Total Times Cited:  72\n"
                "Cited Reference Count:  38"
            ),
            num_cited=72,
            num_references=38,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "product-reformulation",
                "consumer-behavior",
                "economic-analysis",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "longitudinal-data-analysis",
                "validated-dietary-metrics",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Examines anticipatory reformulation effects of the UK SDIL announcement "
                "on British household soft drink purchases."
            ),
            ai_confidence=0.94,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Bandy et al. 2020 - reductions in sugar (Public Health England sugar reduction)
        dict(
            seq=23,
            title="Sugar reduction in soft drinks in the UK from 2015 to 2018 associated with the Soft Drinks Industry Levy",
            abstract_text=(
                "The UK government's Soft Drinks Industry Levy (SDIL) was designed to "
                "encourage manufacturers to reduce the sugar content of soft drinks. We "
                "monitored changes in the sugar content of soft drinks available in the UK "
                "from 2015 to 2018. The proportion of eligible soft drinks with more than "
                "5 g sugar per 100 mL fell from 52% in 2015 to 16% in 2018. The mean sugar "
                "content of eligible drinks fell by 30.4% over the same period. These "
                "changes were largely driven by reformulation rather than changes in market "
                "share. The findings indicate substantial progress in sugar reduction ahead "
                "of the SDIL implementation date."
            ),
            authors=[
                "Bandy, LK",
                "Rayner, M",
                "Jebb, SA",
                "Briggs, ADM",
            ],
            keywords=[
                "Soft drinks industry levy",
                "Sugar reduction",
                "Reformulation",
                "United Kingdom",
                "Public health nutrition",
                "SUGAR-SWEETENED BEVERAGES",
                "SODIUM",
            ],
            journal="PUBLIC HEALTH NUTRITION",
            journal_abbrev="PUBLIC HEALTH NUTR",
            journal_iso_abbrev="Public Health Nutr.",
            doi="10.1017/S1368980020002226",
            issn="1368-9800",
            eissn="1475-2727",
            pub_year=2020,
            volume=23,
            issue=14,
            start_page=2504,
            end_page=2513,
            publisher="CAMBRIDGE UNIV PRESS",
            publisher_city="CAMBRIDGE",
            affiliation="Univ Oxford",
            author_address=(
                "Univ Oxford, Nuffield Dept Populat Hlth, Oxford, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  38\n"
                "Total Times Cited:  52\n"
                "Cited Reference Count:  29"
            ),
            num_cited=52,
            num_references=29,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "product-reformulation",
                "beverages-and-confectionery",
                "packaged-food-supply",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "validated-dietary-metrics",
                "ready-for-data-extraction",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Quantifies UK soft drink sugar reduction and reformulation associated "
                "with the SDIL using product-level data."
            ),
            ai_confidence=0.92,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Amies-Cull et al. 2019 - Lancet Public Health (projected impact of SDIL)
        dict(
            seq=24,
            title="Projected impact of the UK Soft Drinks Industry Levy on childhood and adolescent obesity and health",
            abstract_text=(
                "The UK Soft Drinks Industry Levy (SDIL), announced in March 2016, taxes "
                "sugar-sweetened beverages by sugar concentration. We modelled the potential "
                "health effect of the SDIL on childhood obesity, dental caries, and "
                "long-term diet-related disease in the UK. Using a microsimulation model "
                "based on dietary and health surveillance data, we estimated the SDIL could "
                "prevent approximately 144,000 cases of adult obesity, 5,000 cases of type "
                "2 diabetes, and reduce dental caries among children. The policy is "
                "projected to have the greatest health benefits in lower-income groups, "
                "potentially reducing health inequalities."
            ),
            authors=[
                "Amies-Cull, B",
                "Briggs, ADM",
                "Mytton, OT",
                "Jebb, SA",
                "Cummins, S",
                "Rayner, M",
                "Scarborough, P",
            ],
            keywords=[
                "Soft drink tax",
                "Obesity prevention",
                "Health impact assessment",
                "Microsimulation",
                "United Kingdom",
                "SUGAR-SWEETENED BEVERAGES",
                "POLICY",
            ],
            journal="LANCET PUBLIC HEALTH",
            journal_abbrev="LANCET PUBLIC HEALTH",
            journal_iso_abbrev="Lancet Public Health",
            doi="10.1016/S2468-2667(19)30156-3",
            issn="2468-2667",
            eissn="2468-2667",
            pub_year=2019,
            volume=4,
            issue=10,
            start_page="e500",
            end_page="e508",
            publisher="ELSEVIER SCI LTD",
            publisher_city="OXFORD",
            affiliation="Univ Oxford",
            author_address=(
                "Univ Oxford, Nuffield Dept Populat Hlth, Oxford, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  28\n"
                "Total Times Cited:  40\n"
                "Cited Reference Count:  35"
            ),
            num_cited=40,
            num_references=35,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "health-impacts",
                "beverages-and-confectionery",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "longitudinal-data-analysis",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Models the projected UK health impact of the SDIL on obesity and "
                "diet-related disease outcomes."
            ),
            ai_confidence=0.91,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
        # Gillieson et al. 2023 - added-sugar exposure children UK
        dict(
            seq=25,
            title="Estimated changes in free sugar consumption one year after the UK soft drinks industry levy came into force: controlled interrupted time series analysis of the National Diet and Nutrition Survey (2011-2019)",
            abstract_text=(
                "Objective: To estimate changes in free sugar consumption in the UK "
                "population following the introduction of the Soft Drinks Industry Levy "
                "(SDIL). Design: Controlled interrupted time series analysis of the National "
                "Diet and Nutrition Survey rolling programme (2008/09-2018/19). Setting: "
                "United Kingdom. Participants: Children (aged 1.5-18 years) and adults "
                "(aged 19-95 years). Results: One year post-SDIL, the mean daily free sugar "
                "consumption from soft drinks fell by 3.2 g/day in children and 2.9 g/day "
                "in adults. The proportion of dietary energy from free sugars decreased by "
                "0.5 percentage points in children. Despite these reductions, total free "
                "sugar intake remains above the 5% dietary energy recommendation."
            ),
            authors=[
                "Gillieson, K",
                "Tong, TYN",
                "Pires, SM",
                "Briggs, ADM",
            ],
            keywords=[
                "Soft drinks industry levy",
                "Free sugar consumption",
                "National Diet and Nutrition Survey",
                "Interrupted time series",
                "Dietary intake",
                "SUGAR-SWEETENED BEVERAGES",
                "DIET",
            ],
            journal="JOURNAL OF EPIDEMIOLOGY AND COMMUNITY HEALTH",
            journal_abbrev="J EPIDEMIOL COMMUN H",
            journal_iso_abbrev="J. Epidemiol. Community Health",
            doi="10.1136/jech-2023-224371",
            issn="0143-005X",
            eissn="1470-2738",
            pub_year=2023,
            volume=78,
            issue=9,
            start_page=578,
            end_page=585,
            publisher="BMJ PUBLISHING GROUP",
            publisher_city="LONDON",
            affiliation="Imperial Coll London",
            author_address=(
                "Imperial Coll London, Sch Publ Hlth, London, England"
            ),
            notes=(
                "Times Cited in Web of Science Core Collection:  12\n"
                "Total Times Cited:  18\n"
                "Cited Reference Count:  41"
            ),
            num_cited=18,
            num_references=41,
            status="included",
            tag_names=[
                "food-policy-and-taxation",
                "sugar-and-sweeteners",
                "health-impacts",
                "dietary-exposure-assessment",
                "beverages-and-confectionery",
            ],
            label_names=[
                "sugar-tax-effectiveness",
                "longitudinal-data-analysis",
                "validated-dietary-metrics",
            ],
            ai_decision="include",
            ai_reasoning=(
                "Uses the UK National Diet and Nutrition Survey to estimate SDIL-driven "
                "reductions in free sugar consumption, directly relevant to UK sugar policy."
            ),
            ai_confidence=0.93,
            matched_inclusion=[geo_inc, inclusion_criteria],
        ),
    ]

    new_articles = []
    new_tag_links = []
    new_label_links = []
    new_audits = []

    for kwargs in new_articles_data:
        article, tag_links, label_links, audit = new_article(**kwargs)
        # New articles are manually included - strip ALL AI metadata.
        # Only seq 3 and seq 14 carry AI analysis examples.
        article["aiDecision"] = None
        article["aiConfidence"] = None
        article["aiReasoning"] = None
        article["matchedInclusionCriteria"] = None
        article["matchedExclusionCriteria"] = None
        article["screenedAt"] = None
        article["changedAt"] = ""
        new_articles.append(article)
        new_tag_links.extend(tag_links)
        new_label_links.extend(label_links)
        new_audits.append(audit)

    data["articles"].extend(new_articles)
    data["auditEntries"].extend(new_audits)

    # ── 3. Populate articleTags and articleLabels for EXISTING articles ─
    # These junction tables were empty; map tags/labels based on article content.
    existing_tag_map = {
        1: ["sugar-and-sweeteners", "economic-analysis"],
        2: ["sugar-and-sweeteners", "food-policy-and-taxation", "packaged-food-supply", "product-reformulation"],
        3: ["sugar-and-sweeteners", "economic-analysis"],
        4: ["sugar-and-sweeteners", "economic-analysis"],
        5: ["food-policy-and-taxation", "sugar-and-sweeteners", "product-reformulation", "beverages-and-confectionery", "economic-analysis"],
        6: ["sugar-and-sweeteners", "economic-analysis"],
        7: ["sugar-and-sweeteners", "health-impacts"],
        8: ["sugar-and-sweeteners", "economic-analysis"],
        9: ["sugar-and-sweeteners", "economic-analysis"],
        10: ["food-policy-and-taxation", "sugar-and-sweeteners", "product-reformulation", "consumer-behavior", "beverages-and-confectionery"],
        11: ["food-processing-technology", "industrial-drying"],
        12: ["synthetic-colorants", "sugar-and-sweeteners", "behavioral-disorders", "health-impacts", "beverages-and-confectionery"],
        13: ["natural-pigments", "synthetic-colorants", "food-processing-technology", "stability-and-kinetics"],
        14: ["synthetic-colorants", "sugar-and-sweeteners", "dietary-exposure-assessment", "analytical-methods", "behavioral-disorders"],
        15: ["food-processing-technology", "industrial-drying"],
        16: ["synthetic-colorants", "sugar-and-sweeteners", "behavioral-disorders", "health-impacts"],
        17: ["fishery-and-aquaculture", "sustainability-and-environment"],
        18: ["natural-pigments", "analytical-methods", "stability-and-kinetics", "oxidative-stress"],
    }

    existing_label_map = {
        1: [],
        2: ["validated-dietary-metrics", "ready-for-data-extraction"],
        3: [],
        4: [],
        5: ["sugar-tax-effectiveness", "robust-methodology-did", "longitudinal-data-analysis", "ready-for-data-extraction"],
        6: [],
        7: [],
        8: [],
        9: [],
        10: ["sugar-tax-effectiveness", "longitudinal-data-analysis", "validated-dietary-metrics", "ready-for-data-extraction"],
        11: [],
        12: ["needs-full-text", "validated-dietary-metrics"],
        13: ["needs-full-text"],
        14: ["needs-full-text", "validated-dietary-metrics"],
        15: [],
        16: [],
        17: [],
        18: ["needs-full-text"],
    }

    for seq, aid in EXISTING_IDS.items():
        for tn in existing_tag_map.get(seq, []):
            if tn in TAGS:
                new_tag_links.append({"articleId": aid, "tagId": TAGS[tn]})
        for ln in existing_label_map.get(seq, []):
            if ln in LABELS:
                new_label_links.append({"articleId": aid, "labelId": LABELS[ln]})

    # Deduplicate junction rows (articleId + tagId / labelId pairs)
    seen_tag_pairs = set()
    dedup_tag_links = []
    for link in new_tag_links:
        key = (link["articleId"], link["tagId"])
        if key not in seen_tag_pairs:
            seen_tag_pairs.add(key)
            dedup_tag_links.append(link)

    seen_label_pairs = set()
    dedup_label_links = []
    for link in new_label_links:
        key = (link["articleId"], link["labelId"])
        if key not in seen_label_pairs:
            seen_label_pairs.add(key)
            dedup_label_links.append(link)

    data["articleTags"] = dedup_tag_links
    data["articleLabels"] = dedup_label_links

    # ── 4. Write back ───────────────────────────────────────────────
    output = json.dumps(data, indent=2, ensure_ascii=False)
    DEMO_PATH.write_text(output + "\n", encoding="utf-8")

    # Summary
    included = sum(1 for a in data["articles"] if a["status"] == "included")
    rejected = sum(1 for a in data["articles"] if a["status"] == "rejected")
    duplicates = sum(1 for a in data["articles"] if a["status"] == "duplicate")
    working = sum(1 for a in data["articles"] if a["status"] == "working")
    print(f"Articles: {len(data['articles'])} total")
    print(f"  - included: {included}")
    print(f"  - rejected: {rejected}")
    print(f"  - duplicate: {duplicates}")
    print(f"  - working: {working}")
    print(f"articleTags: {len(data['articleTags'])} links")
    print(f"articleLabels: {len(data['articleLabels'])} links")
    print(f"auditEntries: {len(data['auditEntries'])} entries")


if __name__ == "__main__":
    main()