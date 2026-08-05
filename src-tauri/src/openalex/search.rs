//! URL construction for OpenAlex `/works`. Pure (`#[must_use]`), params percent-encoded.

use super::OpenAlexFilters;

const BASE_URL: &str = "https://api.openalex.org/works";
const SELECT_FIELDS: &str =
    "id,doi,title,authorships,publication_year,publication_date,primary_location,abstract_inverted_index,biblio,cited_by_count,language,keywords,type,open_access,is_retracted";

/// Build filter string. `has_abstract:true` always-on. `is_retracted:false` by default.
#[must_use]
pub fn build_filter_string(filters: &OpenAlexFilters) -> String {
    let mut parts: Vec<String> = vec!["has_abstract:true".to_string()];

    if !filters.show_retracted {
        parts.push("is_retracted:false".to_string());
    }

    if let (Some(from), Some(to)) = (filters.year_from, filters.year_to) {
        parts.push(format!("publication_year:{from}-{to}"));
    } else if let Some(from) = filters.year_from {
        parts.push(format!("publication_year:>{from}"));
    } else if let Some(to) = filters.year_to {
        parts.push(format!("publication_year:<{to}"));
    }

    if !filters.work_types.is_empty() {
        let type_or = filters.work_types.join("|");
        parts.push(format!("type:{type_or}"));
    }

    if let Some(ref lang) = filters.language {
        parts.push(format!("language:{lang}"));
    }

    if filters.is_oa {
        parts.push("is_oa:true".to_string());
    }

    parts.join(",")
}

/// Build the complete `/works` search URL. All params percent-encoded.
#[must_use]
pub fn build_search_url(
    query: &str,
    filters: &OpenAlexFilters,
    sort: &str,
    per_page: u32,
    page: u32,
    mailto: &str,
    api_key: Option<&str>,
) -> String {
    let filter_str = build_filter_string(filters);

    let mut params: Vec<(&str, String)> = vec![
        ("search", query.to_string()),
        ("filter", filter_str.clone()),
        ("sort", sort.to_string()),
        ("per_page", per_page.to_string()),
        ("page", page.to_string()),
        ("select", SELECT_FIELDS.to_string()),
        ("mailto", mailto.to_string()),
    ];

    if let Some(key) = api_key {
        params.push(("api_key", key.to_string()));
    }

    // `parse_with_params` on a constant base URL with valid string params cannot fail.
    match reqwest::Url::parse_with_params(BASE_URL, &params) {
        Ok(url) => url.to_string(),
        Err(_) => format!(
            "{BASE_URL}?search={}&filter={}&sort={}&per_page={}&page={}&select={SELECT_FIELDS}&mailto={}",
            query, filter_str, sort, per_page, page, mailto
        ),
    }
}
