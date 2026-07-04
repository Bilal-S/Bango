# Multilingual Open Access Test Asset Suite

This directory contains open access articles in various languages, used for testing multilingual ingestion, translation, sectioning, and AI processing capabilities.

## Structure
- `manifest.json`: Metadata for all test assets, including expected headings and mappings to test cases.
- `<lang>/<asset-id>.pdf`: The original open access PDF document.
- `<lang>/<asset-id>.ris.json`: Ingest-ready metadata corresponding to the article.

## Asset Table
| Language | DOI | Clean Name | Field | Test Cases |
|---|---|---|---|---|
| `fr` | `10.1016/j.encep.2020.04.008` | `10.1016_j.encep.2020.04.008` | Psychiatry and public health | `TC-02, TC-04, TC-07, TC-08, TC-11, TC-12` |
| `fr` | `10.7202/1002253ar` | `10.7202_1002253ar` | Qualitative research methodology | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `es` | `10.6018/analesps.29.3.178511` | `10.6018_analesps.29.3.178511` | Psychology research methods | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `es` | `none` | `colonialidad-del-poder` | Social sciences and political theory | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `ja` | `10.2169/naika.94.794` | `10.2169_naika.94.794` | Internal medicine | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12, TC-14` |
| `ja` | `10.7210/jrsj.30.830` | `10.7210_jrsj.30.830` | Robotics and software systems | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `zh` | `10.1360/972013-150` | `10.1360_972013-150` | Environmental policy | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `zh` | `10.26549/yzlcyxzz.v4i3.6890` | `10.26549_yzlcyxzz.v4i3.6890` | Clinical medicine | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `de` | `10.1007/bf01797193` | `10.1007_bf01797193` | Neuroscience and medicine | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `de` | `10.1515/znb-1952-0303` | `10.1515_znb-1952-0303` | Microbiology and chemistry | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `ru` | `10.17323/1995-459x.2016.1.31.42` | `10.17323_1995-459x.2016.1.31.42` | Computer science and artificial intelligence | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `ru` | `10.4213/rm358` | `10.4213_rm358` | Mathematics | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `pt` | `10.1590/s0004-282x2003000500014` | `10.1590_s0004-282x2003000500014` | Neurology and mental health | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `pt` | `10.1590/s1415-65552005000400011` | `10.1590_s1415-65552005000400011` | Management research | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `it` | `10.1714/2464.25804` | `10.1714_2464.25804` | Cardiology | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `it` | `10.1007/bf02414525` | `10.1007_bf02414525` | Physics and mechanics | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `ar` | `10.35516/jjba.v21i1.759` | `10.35516_jjba.v21i1.759` | Accounting and corporate governance | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `ar` | `10.31430/ijzh4708` | `10.31430_ijzh4708` | AI policy and governance | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
| `tr` | `10.33400/kuje.843306` | `10.33400_kuje.843306` | Education research methods | `TC-02, TC-03, TC-04, TC-07, TC-11, TC-12` |
| `tr` | `10.32329/uad.711110` | `10.32329_uad.711110` | Online education and pandemic impact | `TC-01, TC-05, TC-06, TC-09, TC-10, TC-13` |
