use crate::error::AppError;
use regex::Regex;
use std::collections::HashMap;

pub struct AffiliationExtractor {
    prefix_re: Regex,
    univ_boundary_re: Regex,
    univ_substring_re: Regex,
    dept_boundary_re: Regex,
    dept_substring_re: Regex,
    normalization_map: HashMap<&'static str, Regex>,
}

impl AffiliationExtractor {
    pub fn new() -> Result<Self, AppError> {
        let boundary_core_terms = [
            "University",
            "Univ",
            "College",
            "Institute",
            "Polytechnic",
            "Academy",
            "Universität",
            "Hochschule",
            "TU",
            "FH",
            "Université",
            "École",
            "Università",
            "Politecnico",
            "Istituto",
            "Collegio",
            "Universidad",
            "Politécnica",
            "Universiteit",
            "Hogeschool",
            "Instituut",
            "Universitet",
            "Högskola",
            "Университет",
            "Академия",
            "Universidade",
            "Uniwersytet",
            "Politechnika",
            "Yliopisto",
            "Korkeakoulu",
            "Univerzita",
            "Vysoká",
            "Høgskole",
            "Høyskole",
            "Üniversite",
            "Enstitü",
            "Universitad",
            "Universitè",
            "Universitât",
            "Istitût",
            "Univerza",
            "Inštitut",
            "Højskole",
        ];

        let substring_core_terms = [
            "대학교",
            "대학",
            "원",
            "विश्वविद्यालय",
            "संस्थान",
            "महाविद्यालय",
            "دانشگاه",
            "پژوهشگاه",
            "موسسه",
        ];

        let boundary_sub_terms = [
            "Department",
            "Dept",
            "School",
            "Sch",
            "Faculty",
            "Laboratory",
            "Lab",
            "Abteilung",
            "Fachbereich",
            "Klinik",
            "Lehrstuhl",
            "Fakultät",
            "Département",
            "Faculté",
            "Laboratoire",
            "Labo",
            "विभाग",
            "संकाय",
            "Dipartimento",
            "Facoltà",
            "Scuola",
            "Depto",
            "Facultad",
            "Escuela",
            "Afdeling",
            "Faculteit",
            "Departement",
            "Institution",
            "Avdelning",
            "Факультет",
            "Кафедра",
            "Отделение",
            "Faculdade",
            "Wydział",
            "Katedra",
            "Laitos",
            "Tiedekunta",
            "Osasto",
            "Fakulta",
            "Ústav",
            "Bölüm",
            "Anabilim",
            "Vakgroep",
            "Departament",
            "Dipartimënt",
            "Facoltât",
            "Oddelek",
        ];

        let substring_sub_terms = ["학과", "학부", "과", "دانشکده", "گروه", "بخش"];

        let prefix_re =
            Regex::new(r"^C3\s+-\s+").map_err(|e| AppError::Validation(e.to_string()))?;

        let univ_boundary_regex_str = format!(r"(?i)\b({})\b", boundary_core_terms.join("|"));
        let univ_boundary_re = Regex::new(&univ_boundary_regex_str)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        let univ_substring_regex_str = format!(r"(?i)({})", substring_core_terms.join("|"));
        let univ_substring_re = Regex::new(&univ_substring_regex_str)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        let dept_boundary_regex_str = format!(r"(?i)\b({})\b", boundary_sub_terms.join("|"));
        let dept_boundary_re = Regex::new(&dept_boundary_regex_str)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        let dept_substring_regex_str = format!(r"(?i)({})", substring_sub_terms.join("|"));
        let dept_substring_re = Regex::new(&dept_substring_regex_str)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        let mut map = HashMap::new();
        // Abbreviation mappings within original languages (no translation of proper nouns)
        map.insert(
            "University",
            Regex::new(r"(?i)\bUniv\.?\b").map_err(|e| AppError::Validation(e.to_string()))?,
        );
        map.insert(
            "Department",
            Regex::new(r"(?i)\bDept\.?\b").map_err(|e| AppError::Validation(e.to_string()))?,
        );
        map.insert(
            "School",
            Regex::new(r"(?i)\bSch\.?\b").map_err(|e| AppError::Validation(e.to_string()))?,
        );

        Ok(Self {
            prefix_re,
            univ_boundary_re,
            univ_substring_re,
            dept_boundary_re,
            dept_substring_re,
            normalization_map: map,
        })
    }

    /// Extract the normalized primary institution name from a single author's address segment.
    pub fn extract(&self, text: &str) -> Option<String> {
        let cleaned_text = self.prefix_re.replace(text, "");
        let segments: Vec<&str> = cleaned_text.split(',').map(|s| s.trim()).collect();

        let mut best_segment = "";
        let mut highest_score = i32::MIN;

        for segment in segments {
            if segment.is_empty() {
                continue;
            }

            let mut score = 0;

            if self.univ_boundary_re.is_match(segment) || self.univ_substring_re.is_match(segment) {
                score += 100;
            }

            if self.dept_boundary_re.is_match(segment) || self.dept_substring_re.is_match(segment) {
                score -= 50;
            }

            if segment.chars().count() <= 3 {
                score -= 10;
            }

            if score > highest_score {
                highest_score = score;
                best_segment = segment;
            }
        }

        if best_segment.is_empty() {
            return None;
        }

        let mut normalized = best_segment.to_string();
        for (replacement, re) in &self.normalization_map {
            normalized = re.replace_all(&normalized, *replacement).to_string();
        }

        Some(normalized)
    }
}
