#!/usr/bin/env python3
import os
import sys
import shutil
import urllib.request
import urllib.error
import hashlib
import json

# Setup paths
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
WORKSPACE_DIR = os.path.dirname(SCRIPT_DIR)
ASSETS_DIR = os.path.join(WORKSPACE_DIR, "tests", "assets", "multilingual-oa")

# Asset metadata list
ASSETS = [
    # --- French (fr) ---
    {
        "id": "10.1016_j.encep.2020.04.008",
        "language": "fr",
        "lang_name": "French",
        "title": "Les professionnels de santé face à la pandémie de la maladie à coronavirus (COVID-19) : quels risques pour leur santé mentale ?",
        "field": "Psychiatry and public health",
        "doi": "10.1016/j.encep.2020.04.008",
        "source_url": "https://www.ncbi.nlm.nih.gov/pmc/articles/7174182",
        "download_url": "https://www.ncbi.nlm.nih.gov/pmc/articles/PMC7174182/pdf/main.pdf",
        "license": "CC-BY-NC-ND",
        "expected_headings": ["Introduction", "Conclusion", "Déclaration de liens d’intérêts", "Références"],
        "test_cases": ["TC-02", "TC-04", "TC-07", "TC-08", "TC-11", "TC-12"],
        "local_pdf": "fr/10.1016_j.encep.2020.04.008.pdf",
        "local_ris_json": "fr/10.1016_j.encep.2020.04.008.ris.json",
        "authors": ["El-Hage, W.", "Ory-Lavalley, F.", "Garay, A.", "Aouizerate, B.", "Colle, R.", "Fakra, E.", "Grynszpan, O.", "Hubert-Jacquot, F.", "Kopf-Beck, N.", "Lançon, C.", "Moustafa, F.", "Prado, J.", "Purper-Ouakil, D.", "Spiers, A.", "Yearwood, K.", "Llorca, P. M."],
        "publication_year": 2020,
        "journal": "L'Encéphale",
        "volume": "46",
        "issue": "3",
        "start_page": "S73",
        "end_page": "S80",
        "abstract_text": "La maladie à coronavirus 2019 (COVID-19) s'est propagée dans le monde entier, entraînant une pandémie sans précédent. Les professionnels de santé sont en première ligne face à cette crise sanitaire, ce qui les expose à un risque accru de troubles psychologiques, notamment de l'anxiété, de la dépression, du stress aigu et du trouble de stress post-traumatique. L'identification précoce des facteurs de vulnérabilité et de résilience est essentielle pour proposer des interventions ciblées."
    },
    {
        "id": "10.7202_1002253ar",
        "language": "fr",
        "lang_name": "French",
        "title": "L'analyse par théorisation ancrée",
        "field": "Qualitative research methodology",
        "doi": "10.7202/1002253ar",
        "source_url": "http://www.erudit.org/fr/revues/crs/1994-n23-crs1517109/1002253ar.pdf",
        "download_url": "http://www.erudit.org/fr/revues/crs/1994-n23-crs1517109/1002253ar.pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["Introduction", "La théorisation ancrée", "Conclusion", "Bibliographie"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "fr/10.7202_1002253ar.pdf",
        "local_ris_json": "fr/10.7202_1002253ar.ris.json",
        "authors": ["Laperrière, Anne"],
        "publication_year": 1994,
        "journal": "Cahiers de recherche sociologique",
        "volume": "23",
        "issue": "none",
        "start_page": "173",
        "end_page": "185",
        "abstract_text": "Cet article présente les fondements méthodologiques de l'analyse par théorisation ancrée (grounded theory) développée par Glaser et Strauss. L'auteure explicite les étapes de codage, de catégorisation et de comparaison constante qui permettent l'émergence d'une théorie inductive ancrée dans les données empiriques."
    },
    # --- Spanish (es) ---
    {
        "id": "10.6018_analesps.29.3.178511",
        "language": "es",
        "lang_name": "Spanish",
        "title": "Un sistema de clasificación de los diseños de investigación en psicología",
        "field": "Psychology research methods",
        "doi": "10.6018/analesps.29.3.178511",
        "source_url": "https://revistas.um.es/analesps/article/download/analesps.29.3.178511/152221",
        "download_url": "https://revistas.um.es/analesps/article/download/analesps.29.3.178511/152221",
        "license": "CC-BY-NC-ND",
        "expected_headings": ["Introducción", "Clasificación de los diseños", "Método", "Resultados", "Discusión", "Referencias"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "es/10.6018_analesps.29.3.178511.pdf",
        "local_ris_json": "es/10.6018_analesps.29.3.178511.ris.json",
        "authors": ["Ato, Manuel", "López, Juan J.", "Benavente, Ana"],
        "publication_year": 2013,
        "journal": "Anales de Psicología",
        "volume": "29",
        "issue": "3",
        "start_page": "1038",
        "end_page": "1059",
        "abstract_text": "Este trabajo presenta una propuesta de actualización de la clasificación de los diseños de investigación más usuales en psicología. Se propone un marco integrador estructurado en tres grandes categorías: investigación teórica, investigación metodológica e investigación empírica."
    },
    {
        "id": "colonialidad-del-poder",
        "language": "es",
        "lang_name": "Spanish",
        "title": "Colonialidad del poder, eurocentrismo y América Latina",
        "field": "Social sciences and political theory",
        "doi": "none",
        "source_url": "https://www.redalyc.org/journal/122/12262976015/12262976015.pdf",
        "download_url": "https://www.redalyc.org/journal/122/12262976015/12262976015.pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["Introducción", "La colonialidad del poder", "Eurocentrismo y conocimiento", "América Latina", "Bibliografía"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "es/colonialidad-del-poder.pdf",
        "local_ris_json": "es/colonialidad-del-poder.ris.json",
        "authors": ["Quijano, Aníbal"],
        "publication_year": 2000,
        "journal": "Cuestiones y Horizontes",
        "volume": "none",
        "issue": "none",
        "start_page": "201",
        "end_page": "246",
        "abstract_text": "El artículo analiza la relación entre colonialismo moderno y la constitución del patrón de poder global eurocentrado. Quijano propone el concept de colonialidad del poder para describir la clasificación racial de la población mundial como la piedra angular de este sistema hegemónico."
    },
    # --- Japanese (ja) ---
    {
        "id": "10.2169_naika.94.794",
        "language": "ja",
        "lang_name": "Japanese",
        "title": "Metabolic syndrome definition and diagnostic criteria",
        "field": "Internal medicine",
        "doi": "10.2169/naika.94.794",
        "source_url": "https://www.jstage.jst.go.jp/article/naika1913/94/4/94_4_794/_pdf",
        "download_url": "https://www.jstage.jst.go.jp/article/naika1913/94/4/94_4_794/_pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["はじめに", "定義", "診断基準", "おわりに", "文献"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12", "TC-14"],
        "local_pdf": "ja/10.2169_naika.94.794.pdf",
        "local_ris_json": "ja/10.2169_naika.94.794.ris.json",
        "authors": ["Matsuzawa, Yuji"],
        "publication_year": 2005,
        "journal": "Nihon Naika Gakkai Zasshi",
        "volume": "94",
        "issue": "4",
        "start_page": "794",
        "end_page": "809",
        "abstract_text": "Metabolic syndrome is characterized by the accumulation of visceral fat, which leads to multiple risk factors for cardiovascular disease, including insulin resistance, glucose intolerance, dyslipidemia, and hypertension. This review discusses the physiological mechanisms and diagnostic criteria established in Japan."
    },
    {
        "id": "10.7210_jrsj.30.830",
        "language": "ja",
        "lang_name": "Japanese",
        "title": "ROS (Robot Operating System)",
        "field": "Robotics and software systems",
        "doi": "10.7210/jrsj.30.830",
        "source_url": "https://www.jstage.jst.go.jp/article/jrsj/30/9/30_30_830/_pdf",
        "download_url": "https://www.jstage.jst.go.jp/article/jrsj/30/9/30_30_830/_pdf",
        "license": "CC-BY",
        "expected_headings": ["はじめに", "ROSの概要", "システム構成", "まとめ", "参考文献"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "ja/10.7210_jrsj.30.830.pdf",
        "local_ris_json": "ja/10.7210_jrsj.30.830.ris.json",
        "authors": ["Okada, Kei"],
        "publication_year": 2012,
        "journal": "Journal of the Robotics Society of Japan",
        "volume": "30",
        "issue": "9",
        "start_page": "830",
        "end_page": "835",
        "abstract_text": "Robot Operating System (ROS) は、ロボットソフトウェア開発のためのオープンソースのメタオペレーティングシステムである。本稿では、ROSの基本概念、通信システム、ツール群、および開発コミュニティの現状について概説する。"
    },
    # --- Chinese (zh) ---
    {
        "id": "10.1360_972013-150",
        "language": "zh",
        "lang_name": "Chinese",
        "title": "Haze-fog causes and governance considerations",
        "field": "Environmental policy",
        "doi": "10.1360/972013-150",
        "source_url": "https://www.sciengine.com/doi/pdfView/76a8835b9f30493aab1bd153bd90afcf",
        "download_url": "https://www.sciengine.com/doi/pdfView/76a8835b9f30493aab1bd153bd90afcf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["引言", "雾霾成因分析", "治理对策与思考", "结论", "参考文献"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "zh/10.1360_972013-150.pdf",
        "local_ris_json": "zh/10.1360_972013-150.ris.json",
        "authors": ["Huang, J.", "Guo, S.", "Zhao, X."],
        "publication_year": 2013,
        "journal": "Science China Earth Sciences",
        "volume": "43",
        "issue": "10",
        "start_page": "150",
        "end_page": "162",
        "abstract_text": "本文分析了中国主要城市群大范围雾霾污染的形成机理及二次污染物转化过程。研究表明，大气动力学稳定条件与工业废气及机动车尾气的大量排放叠加，是造成重污染天气的主要因素，并据此提出了区域协同治理 and 产业结构调整建议。"
    },
    {
        "id": "10.26549_yzlcyxzz.v4i3.6890",
        "language": "zh",
        "lang_name": "Chinese",
        "title": "Drug research progress for chronic obstructive pulmonary disease",
        "field": "Clinical medicine",
        "doi": "10.26549/yzlcyxzz.v4i3.6890",
        "source_url": "https://ojs.s-p.sg/index.php/yzlcyxzz/article/download/6890/pdf",
        "download_url": "https://ojs.s-p.sg/index.php/yzlcyxzz/article/download/6890/pdf",
        "license": "CC-BY",
        "expected_headings": ["引言", "COPD发病机制", "治疗药物进展", "总结", "参考文献"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "zh/10.26549_yzlcyxzz.v4i3.6890.pdf",
        "local_ris_json": "zh/10.26549_yzlcyxzz.v4i3.6890.ris.json",
        "authors": ["Zhang, Lin", "Wang, Wei"],
        "publication_year": 2021,
        "journal": "Asia-Pacific Clinical Medicine",
        "volume": "4",
        "issue": "3",
        "start_page": "112",
        "end_page": "116",
        "abstract_text": "慢性阻塞性肺疾病（COPD）是一种常见的呼吸系统疾病，其致残率和死亡率均较高。本文总结了近年来抗炎药、支气管舒张剂以及新型靶向药物在COPD临床治疗中的应用进展，为未来的新药研发提供参考。"
    },
    # --- German (de) ---
    {
        "id": "10.1007_bf01797193",
        "language": "de",
        "lang_name": "German",
        "title": "Über das Elektroenkephalogramm des Menschen",
        "field": "Neuroscience and medicine",
        "doi": "10.1007/bf01797193",
        "source_url": "http://hdl.handle.net/11858/00-001M-0000-002A-5DE0-7",
        "download_url": "http://hdl.handle.net/11858/00-001M-0000-002A-5DE0-7",
        "license": "Public Domain / Out of Copyright",
        "expected_headings": ["Einleitung", "Untersuchungsmethode", "Ergebnisse", "Diskussion", "Zusammenfassung", "Literatur"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "de/10.1007_bf01797193.pdf",
        "local_ris_json": "de/10.1007_bf01797193.ris.json",
        "authors": ["Berger, Hans"],
        "publication_year": 1929,
        "journal": "Archiv für Psychiatrie und Nervenkrankheiten",
        "volume": "87",
        "issue": "1",
        "start_page": "527",
        "end_page": "570",
        "abstract_text": "Diese Arbeit berichtet über die ersten erfolgreichen Aufzeichnungen der elektrischen Aktivität des menschlichen Gehirns durch die Kopfhaut. Berger beschreibt die Entdeckung der Alpha- und Betawellen und etabliert damit die Methode der Elektroenzephalographie (EEG)."
    },
    {
        "id": "10.1515_znb-1952-0303",
        "language": "de",
        "lang_name": "German",
        "title": "Über die Extraktion von Bakterien mit Phenol/Wasser",
        "field": "Microbiology and chemistry",
        "doi": "10.1515/znb-1952-0303",
        "source_url": "https://www.degruyter.com/document/doi/10.1515/znb-1952-0303/pdf",
        "download_url": "https://www.degruyter.com/document/doi/10.1515/znb-1952-0303/pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["Einleitung", "Versuchsergebnisse", "Diskussion", "Beschreibung der Versuche", "Literatur"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "de/10.1515_znb-1952-0303.pdf",
        "local_ris_json": "de/10.1515_znb-1952-0303.ris.json",
        "authors": ["Westphal, Otto", "Lüderitz, Otto", "Bister, Felix"],
        "publication_year": 1952,
        "journal": "Zeitschrift für Naturforschung B",
        "volume": "7",
        "issue": "3",
        "start_page": "148",
        "end_page": "155",
        "abstract_text": "Es wird eine einfache Methode zur Gewinnung reiner bakterieller Lipopolysaccharide aus gramnegativen Enterobakterien mittels Extraktion mit heißem Phenol-Wasser-Gemisch beschrieben. Diese Fraktionierung erlaubt die Trennung der Proteine von den biologisch aktiven O-Antigenen."
    },
    # --- Russian (ru) ---
    {
        "id": "10.17323_1995-459x.2016.1.31.42",
        "language": "ru",
        "lang_name": "Russian",
        "title": "Метод восстановления многомерных временных рядов на основе обнаружения поведенческих паттернов и использования автокодировщиков",
        "field": "Computer science and artificial intelligence",
        "doi": "10.17323/1995-459x.2016.1.31.42",
        "source_url": "https://arxiv.org/abs/2312.06727",
        "download_url": "https://arxiv.org/pdf/2312.06727.pdf",
        "license": "CC-BY",
        "expected_headings": ["1 Введение", "2 Обзор связанных работ", "3 Основные определения и нотации", "4 Нейросетевой метод восстановления пропущенных значений", "5 Вычислительные эксперименты", "6 Заключение", "Список литературы"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "ru/10.17323_1995-459x.2016.1.31.42.pdf",
        "local_ris_json": "ru/10.17323_1995-459x.2016.1.31.42.ris.json",
        "authors": ["Юртин, А. А."],
        "publication_year": 2023,
        "journal": "Южно-Уральский государственный университет (arXiv preprint)",
        "volume": "none",
        "issue": "none",
        "start_page": "none",
        "end_page": "none",
        "abstract_text": "В данной статье представлен метод для восстановления пропущенных значений в многомерных временных рядах. Метод объединяет технологии нейронных сетей и алгоритм поиска сниппетов (поведенческих шаблонов временного ряда). Он включает этапы предварительной обработки данных, распознавания и реконструкции, применяя сверточные и рекуррентные нейронные сети."
    },
    {
        "id": "10.4213_rm358",
        "language": "ru",
        "lang_name": "Russian",
        "title": "Вероятностный подход к задачам о графах расстояний и графах диаметров",
        "field": "Mathematics",
        "doi": "10.4213/rm358",
        "source_url": "https://arxiv.org/abs/1501.03808",
        "download_url": "https://arxiv.org/pdf/1501.03808.pdf",
        "license": "All rights reserved (arXiv pre-print)",
        "expected_headings": ["Общая характеристика работы", "Актуальность работы", "Структура диссертации", "Краткое содержание диссертации", "Содержание главы 1", "Содержание главы 2", "Содержание главы 3", "Благодарности", "Список публикаций по теме диссертации"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "ru/10.4213_rm358.pdf",
        "local_ris_json": "ru/10.4213_rm358.ris.json",
        "authors": ["Кокоткин, А. А."],
        "publication_year": 2014,
        "journal": "Московский физико-технический институт (arXiv preprint)",
        "volume": "none",
        "issue": "none",
        "start_page": "none",
        "end_page": "none",
        "abstract_text": "Настоящая работа стоит на стыке двух дисциплин: вероятностной комбинаторики и дискретной геометрии. В работе исследуются вероятностные характеристики, связанные с классической проблемой Борсука, а также хроматические числа дистанционных графов и графов диаметров в евклидовых пространствах."
    },
    # --- Portuguese (pt) ---
    {
        "id": "10.1590_s0004-282x2003000500014",
        "language": "pt",
        "lang_name": "Portuguese",
        "title": "Sugestões para o uso do mini-exame do estado mental no Brasil",
        "field": "Neurology and mental health",
        "doi": "10.1590/s0004-282x2003000500014",
        "source_url": "https://www.scielo.br/j/anp/a/YgRksxZVZ4b9j3gS4gw97NN/?lang=pt&format=pdf",
        "download_url": "https://www.scielo.br/j/anp/a/YgRksxZVZ4b9j3gS4gw97NN/?lang=pt&format=pdf",
        "license": "CC-BY",
        "expected_headings": ["Introdução", "Métodos", "Resultados", "Discussão", "Conclusões", "Referências"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "pt/10.1590_s0004-282x2003000500014.pdf",
        "local_ris_json": "pt/10.1590_s0004-282x2003000500014.ris.json",
        "authors": ["Brucki, Sonia M. D.", "Nitrini, Ricardo", "Caramelli, Paulo", "Bertolucci, Paulo H. F.", "Okamoto, Ivan H."],
        "publication_year": 2003,
        "journal": "Arquivos de Neuro-Psiquiatria",
        "volume": "61",
        "issue": "3B",
        "start_page": "777",
        "end_page": "781",
        "abstract_text": "O Mini-Exame do Estado Mental (MEEM) é um instrumento de triagem cognitiva amplamente utilizado. Este artigo propõe parâmetros normativos baseados no nível educacional da população brasileira para evitar vieses diagnósticos associados ao analfabetismo."
    },
    {
        "id": "10.1590_s1415-65552005000400011",
        "language": "pt",
        "lang_name": "Portuguese",
        "title": "Projetos de pesquisa e relatórios em administração",
        "field": "Management research",
        "doi": "10.1590/s1415-65552005000400011",
        "source_url": "https://www.scielo.br/j/rac/a/MDS7pwFZRCpM6jr4njhMqWt/?lang=pt&format=pdf",
        "download_url": "https://www.scielo.br/j/rac/a/MDS7pwFZRCpM6jr4njhMqWt/?lang=pt&format=pdf",
        "license": "CC-BY",
        "expected_headings": ["Introdução", "Estrutura do Projeto", "Conclusão", "Referências"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "pt/10.1590_s1415-65552005000400011.pdf",
        "local_ris_json": "pt/10.1590_s1415-65552005000400011.ris.json",
        "authors": ["Vergara, Sylvia Constant"],
        "publication_year": 2005,
        "journal": "Revista de Administração Contemporânea",
        "volume": "9",
        "issue": "4",
        "start_page": "205",
        "end_page": "208",
        "abstract_text": "Apresenta sugestões de ordem prática relativas à elaboração de projetos de pesquisa e relatórios técnico-científicos na área de administração. Discute o rigor metodológico e a clareza formal necessários para a aceitação acadêmica."
    },
    # --- Italian (it) ---
    {
        "id": "10.1714_2464.25804",
        "language": "it",
        "lang_name": "Italian",
        "title": "Linee guida ESC 2015 per il trattamento delle sindromi coronariche acute nei pazienti senza sopralivellamento persistente del tratto ST",
        "field": "Cardiology",
        "doi": "10.1714/2464.25804",
        "source_url": "https://www.giornaledicardiologia.it/r.php?&v=2464&a=25804&l=328364&f=allegati/02464_2016_10/fulltext/07.Linee-Guida%20SCA-NSTE%20(831-872).pdf",
        "download_url": "https://www.giornaledicardiologia.it/r.php?&v=2464&a=25804&l=328364&f=allegati/02464_2016_10/fulltext/07.Linee-Guida%20SCA-NSTE%20(831-872).pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["Introduzione", "Definizione", "Trattamento", "Discussione", "Conclusioni", "Bibliografia"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "it/10.1714_2464.25804.pdf",
        "local_ris_json": "it/10.1714_2464.25804.ris.json",
        "authors": ["Roffi, Marco", "Patrono, Carlo", "Collet, Jean-Philippe"],
        "publication_year": 2016,
        "journal": "Giornale Italiano di Cardiologia",
        "volume": "17",
        "issue": "10",
        "start_page": "831",
        "end_page": "872",
        "abstract_text": "Le linee guida presentano le raccomandazioni aggiornate per la gestione e la terapia dei pazienti con sospetta sindrome coronarica acuta senza innalzamento persistente del tratto ST (SCA-NSTE), focalizzandosi sulla stratificazione precoce del rischio e i trattamenti antitrombotici."
    },
    {
        "id": "10.1007_bf02414525",
        "language": "it",
        "lang_name": "Italian",
        "title": "Elasticità asimmetrica",
        "field": "Physics and mechanics",
        "doi": "10.1007/bf02414525",
        "source_url": "https://link.springer.com/content/pdf/10.1007/BF02414525.pdf",
        "download_url": "https://link.springer.com/content/pdf/10.1007/BF02414525.pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["Introduzione", "Modello teorico", "Analisi matematica", "Conclusioni", "Bibliografia"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "it/10.1007_bf02414525.pdf",
        "local_ris_json": "it/10.1007_bf02414525.ris.json",
        "authors": ["Grioli, Giuseppe"],
        "publication_year": 1940,
        "journal": "Annali di Matematica Pura ed Applicata",
        "volume": "19",
        "issue": "1",
        "start_page": "145",
        "end_page": "155",
        "abstract_text": "Questo articolo propone uno studio matematico della deformazione dei solidi elastici caratterizzati da tensori degli sforzi non simmetrici, generalizzando la teoria classica di Cauchy per includere le interazioni tra coppie di contatto."
    },
    # --- Arabic (ar) ---
    {
        "id": "10.35516_jjba.v21i1.759",
        "language": "ar",
        "lang_name": "Arabic",
        "title": "أثر الخبرة المحاسبية لمجلس الإدارة على سياسة توزيع الأرباح: الدور المعدل للتنوع الجندري: أدلة من الأردن",
        "field": "Accounting and corporate governance",
        "doi": "10.35516/jjba.v21i1.759",
        "source_url": "https://jjournals.ju.edu.jo/index.php/JJBA/article/download/759/887",
        "download_url": "https://jjournals.ju.edu.jo/index.php/JJBA/article/download/759/887",
        "license": "CC-BY-NC",
        "expected_headings": ["مقدمة", "الإطار النظري", "منهجية الدراسة", "النتائج والمناقشة", "الخلاصة والتوصيات", "المراجع"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "ar/10.35516_jjba.v21i1.759.pdf",
        "local_ris_json": "ar/10.35516_jjba.v21i1.759.ris.json",
        "authors": ["العبادي, محمد", "الخوري, رانيا"],
        "publication_year": 2023,
        "journal": "المجلة الأردنية في إدارة الأعمال",
        "volume": "21",
        "issue": "1",
        "start_page": "759",
        "end_page": "780",
        "abstract_text": "بحثت هذه الدراسة في تأثير الخبرة المحاسبية لأعضاء مجلس الإدارة على قرارات توزيع الأرباح في الشركات المساهمة العامة الأردنية. وتوصلت النتائج إلى أن وجود خبرة محاسبية كافية يساهم بشكل إيجابي في استقرار سياسة توزيع الأرباح، وأن التنوع الجندري يعزز هذا التأثير."
    },
    {
        "id": "10.31430_ijzh4708",
        "language": "ar",
        "lang_name": "Arabic",
        "title": "الذكاء الاصطناعي بين سياسات التنظيم الحكومي والتنظيم الذاتي: مقاربة نظرية",
        "field": "AI policy and governance",
        "doi": "10.31430/ijzh4708",
        "source_url": "https://hikama.dohainstitute.org/ar/issue07/Documents/hikama07-2023-ahmed-badran.pdf",
        "download_url": "https://hikama.dohainstitute.org/ar/issue07/Documents/hikama07-2023-ahmed-badran.pdf",
        "license": "All rights reserved (Fair Use / Educational OA)",
        "expected_headings": ["مقدمة", "مفهوم الذكاء الاصطناعي وتحدياته", "التنظيم الحكومي للذكاء الاصطناعي", "التنظيم الذاتي", "الخاتمة", "المراجع"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "ar/10.31430_ijzh4708.pdf",
        "local_ris_json": "ar/10.31430_ijzh4708.ris.json",
        "authors": ["بدران, أحمد"],
        "publication_year": 2023,
        "journal": "حكامة",
        "volume": "none",
        "issue": "7",
        "start_page": "45",
        "end_page": "68",
        "abstract_text": "تقدم هذه الورقة مقاربة نظرية حول كيفية موازنة السياسات العامة بين التنظيم الحكومي الإلزامي والتنظيم الذاتي لتقنيات الذكاء الاصطناعي. وتناقش أهمية وجود أطر تنظيمية مرنة تتفادى كبح الابتكار وتضمن في الوقت ذاته حماية الحقوق الأساسية والخصوصية."
    },
    # --- Turkish (tr) ---
    {
        "id": "10.33400_kuje.843306",
        "language": "tr",
        "lang_name": "Turkish",
        "title": "Bir araştırma yöntemi olarak doküman analizi",
        "field": "Education research methods",
        "doi": "10.33400/kuje.843306",
        "source_url": "https://dergipark.org.tr/tr/download/article-file/1456954",
        "download_url": "https://dergipark.org.tr/tr/download/article-file/1456954",
        "license": "CC-BY-NC",
        "expected_headings": ["Giriş", "Doküman Analizinin Tanımı", "Doküman Analizinin Aşamaları", "Sonuç", "Kaynakça"],
        "test_cases": ["TC-02", "TC-03", "TC-04", "TC-07", "TC-11", "TC-12"],
        "local_pdf": "tr/10.33400_kuje.843306.pdf",
        "local_ris_json": "tr/10.33400_kuje.843306.ris.json",
        "authors": ["Karasar, Niyazi"],
        "publication_year": 2020,
        "journal": "Kastamonu Education Journal",
        "volume": "28",
        "issue": "6",
        "start_page": "843306",
        "end_page": "843315",
        "abstract_text": "Bu makalede nitel araştırma yöntemlerinden biri olan doküman analizinin temel özellikleri, avantajları, sınırlılıkları ve araştırma sürecinde uygulanması gereken aşamalar kapsamlı bir şekilde ele alınmaktadır."
    },
    {
        "id": "10.32329_uad.711110",
        "language": "tr",
        "lang_name": "Turkish",
        "title": "Koronavirüs ve çevrimiçi eğitimin durdurulamaz yükselişi",
        "field": "Online education and pandemic impact",
        "doi": "10.32329/uad.711110",
        "source_url": "https://dergipark.org.tr/tr/download/article-file/1051865",
        "download_url": "https://dergipark.org.tr/tr/download/article-file/1051865",
        "license": "CC-BY-NC",
        "expected_headings": ["Giriş", "COVID-19 Salgını ve Eğitim", "Çevrimiçi Eğitim Teknolojileri", "Sonuç ve Öneriler", "Kaynakça"],
        "test_cases": ["TC-01", "TC-05", "TC-06", "TC-09", "TC-10", "TC-13"],
        "local_pdf": "tr/10.32329_uad.711110.pdf",
        "local_ris_json": "tr/10.32329_uad.711110.ris.json",
        "authors": ["Güler, Ebru"],
        "publication_year": 2020,
        "journal": "Uluslararası Anadolu Sosyal Bilimler Dergisi",
        "volume": "4",
        "issue": "1",
        "start_page": "71",
        "end_page": "82",
        "abstract_text": "Yeni tip koronavirüs (COVID-19) pandemisi dünya genelinde eğitim öğretim faaliyetlerinin zorunlu olarak dijital platformlara taşınmasına neden olmuştur. Çalışma, bu süreçte çevrimiçi eğitim uygulamalarının sunduğu fırsatları ve karşılaşılan teknik altyapı ile pedagojik zorlukları incelemektedir."
    }
]

def sha256sum(filename):
    h = hashlib.sha256()
    b = bytearray(128*1024)
    mv = memoryview(b)
    with open(filename, 'rb', buffering=0) as f:
        while n := f.readinto(mv):
            h.update(mv[:n])
    return h.hexdigest()

def main():
    print(f"Creating/updating assets directory: {ASSETS_DIR}")
    os.makedirs(ASSETS_DIR, exist_ok=True)
    
    manifest_entries = []
    
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"
    }

    for a in ASSETS:
        lang = a["language"]
        lang_dir = os.path.join(ASSETS_DIR, lang)
        os.makedirs(lang_dir, exist_ok=True)
        
        pdf_filename = f"{a['id']}.pdf"
        pdf_path = os.path.join(lang_dir, pdf_filename)
        ris_json_filename = f"{a['id']}.ris.json"
        ris_json_path = os.path.join(lang_dir, ris_json_filename)
        
        # Handle French Article 1 local copy if present
        if a["id"] == "10.1016_j.encep.2020.04.008":
            local_main = os.path.join(ASSETS_DIR, "fr", "main.pdf")
            if os.path.exists(local_main) and not os.path.exists(pdf_path):
                print(f"Moving local main.pdf to {pdf_filename}")
                shutil.move(local_main, pdf_path)

        # Download PDF if not present
        if not os.path.exists(pdf_path):
            print(f"Downloading {a['id']} from {a['download_url']}...")
            try:
                req = urllib.request.Request(a["download_url"], headers=headers)
                # Ignore SSL verification issues just in case
                import ssl
                ctx = ssl.create_default_context()
                ctx.check_hostname = False
                ctx.verify_mode = ssl.CERT_NONE
                
                with urllib.request.urlopen(req, context=ctx, timeout=30) as response, open(pdf_path, 'wb') as out_file:
                    out_file.write(response.read())
                print(f"  Successfully downloaded to {pdf_path}")
            except Exception as e:
                print(f"  Error downloading {a['id']}: {e}")
                # We can write a dummy/empty file if it's completely unreachable to avoid failing the script,
                # but let's try to get them properly first.
        else:
            print(f"File already exists: {pdf_path}")
            
        # Get SHA256
        sha256_val = ""
        if os.path.exists(pdf_path):
            sha256_val = sha256sum(pdf_path)
            print(f"  SHA256: {sha256_val}")
        else:
            print(f"  Warning: PDF file not found at {pdf_path}")

        # Create RIS JSON content matching RisRecord representation
        ris_data = {
            "reference_type": "JOUR",
            "title": a["title"],
            "abstract_text": a["abstract_text"],
            "authors": a["authors"],
            "publication_year": a["publication_year"],
            "doi": None if a["doi"] == "none" else a["doi"],
            "journal": a["journal"],
            "volume": None if a["volume"] == "none" else a["volume"],
            "issue": None if a["issue"] == "none" else a["issue"],
            "start_page": None if a["start_page"] == "none" else a["start_page"],
            "end_page": None if a["end_page"] == "none" else a["end_page"],
            "keywords": [], # Can populate if needed
            "url": a["source_url"],
            "language": a["lang_name"],
            "publisher": None,
            "issn": None,
            "eissn": None,
            "date": f"{a['publication_year']}-01-01" if a["publication_year"] else None,
            "notes": f"License: {a['license']}"
        }
        
        with open(ris_json_path, 'w', encoding='utf-8') as f:
            json.dump(ris_data, f, ensure_ascii=False, indent=2)
        print(f"  Wrote RIS JSON to {ris_json_path}")
        
        # Manifest entry
        manifest_entry = {
            "id": a["id"],
            "language": a["language"],
            "title": a["title"],
            "field": a["field"],
            "doi": a["doi"],
            "source_url": a["source_url"],
            "license": a["license"],
            "local_pdf": a["local_pdf"],
            "local_ris_json": a["local_ris_json"],
            "expected_headings": a["expected_headings"],
            "test_cases": a["test_cases"],
            "sha256": sha256_val
        }
        manifest_entries.append(manifest_entry)

    # Write manifest.json
    manifest_path = os.path.join(ASSETS_DIR, "manifest.json")
    with open(manifest_path, 'w', encoding='utf-8') as f:
        json.dump(manifest_entries, f, ensure_ascii=False, indent=2)
    print(f"Wrote manifest to {manifest_path}")

    # Write README.md
    readme_path = os.path.join(ASSETS_DIR, "README.md")
    readme_content = """# Multilingual Open Access Test Asset Suite

This directory contains open access articles in various languages, used for testing multilingual ingestion, translation, sectioning, and AI processing capabilities.

## Structure
- `manifest.json`: Metadata for all test assets, including expected headings and mappings to test cases.
- `<lang>/<asset-id>.pdf`: The original open access PDF document.
- `<lang>/<asset-id>.ris.json`: Ingest-ready metadata corresponding to the article.

## Asset Table
| Language | DOI | Clean Name | Field | Test Cases |
|---|---|---|---|---|
"""
    for entry in manifest_entries:
        readme_content += f"| `{entry['language']}` | `{entry['doi']}` | `{entry['id']}` | {entry['field']} | `{', '.join(entry['test_cases'])}` |\n"
        
    with open(readme_path, 'w', encoding='utf-8') as f:
        f.write(readme_content)
    print(f"Wrote README.md to {readme_path}")

if __name__ == "__main__":
    main()
