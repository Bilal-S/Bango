use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RisExportArticle {
    pub reference_type: Option<String>,
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub start_page: Option<String>,
    pub end_page: Option<String>,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub issn: Option<String>,
    pub ai_reasoning: Option<String>,
    pub user_notes: Option<String>,
    pub ai_decision: Option<String>,
    pub labels: Vec<String>,
}

#[must_use]
pub fn article_to_ris(article: &RisExportArticle) -> String {
    let mut lines = Vec::new();

    lines.push(format!("TY  - {}", article.reference_type.as_deref().unwrap_or("JOUR")));
    lines.push(format!("TI  - {}", article.title));
    lines.push(format!("AB  - {}", article.abstract_text));

    for author in &article.authors {
        lines.push(format!("AU  - {}", author));
    }

    if let Some(year) = article.publication_year {
        lines.push(format!("PY  - {}", year));
    }
    if let Some(ref doi) = article.doi {
        lines.push(format!("DO  - {}", doi));
    }
    if let Some(ref journal) = article.journal {
        lines.push(format!("T2  - {}", journal));
    }
    if let Some(ref vol) = article.volume {
        lines.push(format!("VL  - {}", vol));
    }
    if let Some(ref issue) = article.issue {
        lines.push(format!("IS  - {}", issue));
    }
    if let Some(ref sp) = article.start_page {
        lines.push(format!("SP  - {}", sp));
    }
    if let Some(ref ep) = article.end_page {
        lines.push(format!("EP  - {}", ep));
    }

    for kw in &article.keywords {
        lines.push(format!("KW  - {}", kw));
    }
    for tag in &article.tags {
        lines.push(format!("KW  - {}", tag));
    }

    if let Some(ref url) = article.url {
        lines.push(format!("UR  - {}", url));
    }
    if let Some(ref lang) = article.language {
        lines.push(format!("LA  - {}", lang));
    }
    if let Some(ref pub_) = article.publisher {
        lines.push(format!("PB  - {}", pub_));
    }
    if let Some(ref issn) = article.issn {
        lines.push(format!("SN  - {}", issn));
    }
    if let Some(ref reasoning) = article.ai_reasoning {
        lines.push(format!("N1  - {}", reasoning));
    }
    if let Some(ref notes) = article.user_notes {
        lines.push(format!("NO  - {}", notes));
    }

    // Labels grouped by decision
    if !article.labels.is_empty() {
        let decision = article.ai_decision.as_deref().unwrap_or("include");
        let (inc, exc): (Vec<_>, Vec<_>) = if decision == "include" {
            (article.labels.clone(), vec![])
        } else {
            (vec![], article.labels.clone())
        };

        let inc_json = serde_json::to_string(&inc).unwrap_or_default();
        let exc_json = serde_json::to_string(&exc).unwrap_or_default();
        lines.push(format!("C1  - {{\"inc\":{},\"exc\":{}}}", inc_json, exc_json));
    }

    lines.push("ER  -".to_string());

    lines.join("\n") + "\n"
}

#[must_use]
pub fn articles_to_ris(articles: &[RisExportArticle]) -> String {
    articles.iter().map(article_to_ris).collect()
}
