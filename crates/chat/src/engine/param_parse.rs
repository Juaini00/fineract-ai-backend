//! Deterministic extraction of period, limit, and currency straight from the
//! request text (FIN-135).
//!
//! No LLM: every function here is pure text matching over the words a request
//! already contains. A request that states nothing recognizable returns
//! `None`, and the caller falls back to the manifest's declared default
//! unchanged — this module never invents a value the text does not carry.
//!
//! Matching is generic over parameter **names** that already carry
//! cross-capability meaning (`from_date`/`to_date`, `limit`, `currency_code`),
//! never over which capability was selected (no phrase allowlist per
//! capability).

use chrono::{Datelike, NaiveDate};

/// Split into words the same way [`super::planner::lexical_terms`] does, but
/// preserving order and original casing: phrase reconstruction and
/// title-case detection both need the source text, not a normalized set.
fn words(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn month_number(word: &str) -> Option<u32> {
    match word {
        "january" | "jan" | "januari" => Some(1),
        "february" | "feb" | "februari" => Some(2),
        "march" | "mar" | "maret" => Some(3),
        "april" | "apr" => Some(4),
        "may" | "mei" => Some(5),
        "june" | "jun" | "juni" => Some(6),
        "july" | "jul" | "juli" => Some(7),
        "august" | "aug" | "agustus" | "agu" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" | "oktober" | "okt" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" | "desember" | "des" => Some(12),
        _ => None,
    }
}

fn parse_year_token(word: &str) -> Option<i32> {
    word.parse::<i32>()
        .ok()
        .filter(|year| (1900..=2100).contains(year))
}

/// Last day of `year`/`month`. `chrono` has no direct "end of month"; the
/// standard trick is one day before the first of the next month.
fn end_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)?.pred_opt()
}

/// `business_today - <N> months`, same rule [`super::planner::relative_date`]
/// uses: never produces a date that does not exist (31 Mar - 1m -> 28/29 Feb).
pub(super) fn subtract_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = date.year() as i64 * 12 + (date.month() as i64 - 1) - months;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = total.rem_euclid(12) as u32 + 1;

    (0..4).find_map(|back| NaiveDate::from_ymd_opt(year, month, date.day().checked_sub(back)?))
}

/// A period the request text states explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPeriod {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Human-readable phrase the period was derived from, for disclosure.
    pub detail: String,
}

/// Try, in order: an explicit month range ("January to September 2026"), a
/// relative "last N months" / "N bulan terakhir", a bare year ("for 2026"),
/// then single relative-day phrases ("today"/"hari ini", …).
///
/// The end of an open-ended period (a month/year that includes today) is
/// clamped to `today`: a whole future month or year has no data yet, and
/// answering with one would silently claim rows that do not exist.
pub fn parse_period(text: &str, today: NaiveDate) -> Option<ParsedPeriod> {
    parse_month_range(text, today)
        .or_else(|| parse_last_n_months(text, today))
        .or_else(|| parse_year_only(text, today))
        .or_else(|| parse_relative_phrase(text, today))
}

const RANGE_CONNECTORS: [&str; 5] = ["to", "through", "until", "sampai", "hingga"];

fn parse_month_range(text: &str, today: NaiveDate) -> Option<ParsedPeriod> {
    let words = words(text);
    let lower: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();

    for start in 0..lower.len() {
        let Some(month1) = month_number(&lower[start]) else {
            continue;
        };
        let mut cursor = start + 1;
        let year1 = parse_year_token(lower.get(cursor)?.as_str()).inspect(|_| cursor += 1);

        let connector = (cursor..lower.len().min(cursor + 2))
            .find(|&index| RANGE_CONNECTORS.contains(&lower[index].as_str()));
        let Some(connector) = connector else { continue };
        cursor = connector + 1;

        let Some(month2) = lower.get(cursor).and_then(|word| month_number(word)) else {
            continue;
        };
        cursor += 1;
        let year2 = lower
            .get(cursor)
            .and_then(|word| parse_year_token(word))
            .inspect(|_| cursor += 1);

        let year2 = year2.or(year1).unwrap_or_else(|| today.year());
        let year1 = year1.unwrap_or(year2);

        let from = NaiveDate::from_ymd_opt(year1, month1, 1)?;
        let to = end_of_month(year2, month2)?.min(today);
        let detail = words[start..cursor].join(" ");
        return Some(ParsedPeriod { from, to, detail });
    }
    None
}

fn parse_last_n_months(text: &str, today: NaiveDate) -> Option<ParsedPeriod> {
    let words = words(text);
    let lower: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();

    for index in 0..lower.len() {
        // "last N months"
        // Same exact-day subtraction as the catalog's own `business_today -
        // Nm` default expression (`relative_date`): a phrase that names the
        // same span as an existing default must bind the identical value, or
        // "the last 12 months" and a capability's own `business_today - 12m`
        // default would silently disagree about what that phrase means.
        if lower[index] == "last"
            && let Some(count) = lower
                .get(index + 1)
                .and_then(|word| word.parse::<i64>().ok())
            && matches!(
                lower.get(index + 2).map(String::as_str),
                Some("months") | Some("month")
            )
        {
            let from = subtract_months(today, count)?;
            return Some(ParsedPeriod {
                from,
                to: today,
                detail: format!("last {count} months"),
            });
        }
        // "N bulan terakhir"
        if let Ok(count) = lower[index].parse::<i64>()
            && lower.get(index + 1).map(String::as_str) == Some("bulan")
            && lower.get(index + 2).map(String::as_str) == Some("terakhir")
        {
            let from = subtract_months(today, count)?;
            return Some(ParsedPeriod {
                from,
                to: today,
                detail: format!("{count} bulan terakhir"),
            });
        }
    }
    None
}

const YEAR_CONTEXT: [&str; 6] = ["for", "in", "during", "untuk", "di", "sepanjang"];

fn parse_year_only(text: &str, today: NaiveDate) -> Option<ParsedPeriod> {
    let words = words(text);
    let lower: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();

    for index in 1..lower.len() {
        let Some(year) = parse_year_token(&lower[index]) else {
            continue;
        };
        // A year right after a month name belongs to `parse_month_range`,
        // already tried first; do not double-match it here as a whole year.
        if month_number(&lower[index - 1]).is_some() {
            continue;
        }
        if !YEAR_CONTEXT.contains(&lower[index - 1].as_str()) {
            continue;
        }

        let from = NaiveDate::from_ymd_opt(year, 1, 1)?;
        let to = end_of_month(year, 12)?.min(today);
        return Some(ParsedPeriod {
            from,
            to,
            detail: format!("{} {}", words[index - 1], words[index]),
        });
    }
    None
}

fn parse_relative_phrase(text: &str, today: NaiveDate) -> Option<ParsedPeriod> {
    let joined = format!(" {} ", words(text).join(" ").to_lowercase());
    let has_phrase = |phrase: &str| joined.contains(&format!(" {phrase} "));

    if has_phrase("hari ini") || has_phrase("today") {
        return Some(ParsedPeriod {
            from: today,
            to: today,
            detail: "today".to_string(),
        });
    }
    if has_phrase("kemarin") || has_phrase("yesterday") {
        let day = today.pred_opt().unwrap_or(today);
        return Some(ParsedPeriod {
            from: day,
            to: day,
            detail: "yesterday".to_string(),
        });
    }
    if has_phrase("minggu ini") || has_phrase("this week") {
        let from = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
        return Some(ParsedPeriod {
            from,
            to: today,
            detail: "this week".to_string(),
        });
    }
    if has_phrase("bulan ini") || has_phrase("this month") {
        let from = today.with_day(1).unwrap_or(today);
        return Some(ParsedPeriod {
            from,
            to: today,
            detail: "this month".to_string(),
        });
    }
    None
}

/// A row-count limit ("top N" / "N terbesar") the request text states
/// explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLimit {
    pub value: i64,
    pub detail: String,
}

pub fn parse_limit(text: &str) -> Option<ParsedLimit> {
    let words = words(text);
    let lower: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();

    for index in 0..lower.len() {
        if lower[index] == "top"
            && let Some(value) = lower
                .get(index + 1)
                .and_then(|word| word.parse::<i64>().ok())
        {
            return Some(ParsedLimit {
                value,
                detail: format!("top {value}"),
            });
        }
        if let Ok(value) = lower[index].parse::<i64>()
            && let Some(offset) = (1..=3usize).find(|&offset| {
                matches!(
                    lower.get(index + offset).map(String::as_str),
                    Some("terbesar") | Some("teratas") | Some("tertinggi")
                )
            })
        {
            return Some(ParsedLimit {
                value,
                detail: format!("{value} {}", lower[index + offset]),
            });
        }
    }
    None
}

/// An ISO currency code the request text states explicitly — either the code
/// itself or a common name for it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCurrency {
    pub code: String,
    pub detail: String,
}

const CURRENCY_CODES: [&str; 9] = [
    "idr", "usd", "eur", "sgd", "myr", "jpy", "gbp", "aud", "cny",
];

pub fn parse_currency(text: &str) -> Option<ParsedCurrency> {
    for word in words(text) {
        let lower = word.to_lowercase();
        if CURRENCY_CODES.contains(&lower.as_str()) {
            return Some(ParsedCurrency {
                code: lower.to_uppercase(),
                detail: word.to_string(),
            });
        }
        let code = match lower.as_str() {
            "rupiah" => Some("IDR"),
            "dollar" | "dolar" => Some("USD"),
            "euro" => Some("EUR"),
            _ => None,
        };
        if let Some(code) = code {
            return Some(ParsedCurrency {
                code: code.to_string(),
                detail: word.to_string(),
            });
        }
    }
    None
}

const ENTITY_PREPOSITIONS: [&str; 8] = ["from", "at", "in", "of", "for", "dari", "di", "pada"];

fn is_title_case(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => chars.all(|c| c.is_lowercase()),
        _ => false,
    }
}

/// A capitalized phrase the text names after a generic preposition, e.g.
/// "from **Head Office**" / "dari **Kantor Pusat**" — a candidate entity
/// value for an identity-like slot that has no default and no resolver.
///
/// Deliberately conservative: single all-caps tokens (currency codes),
/// month names, and sentence-initial words never match, so this never fires
/// on a period or currency the other parsers already consumed.
pub fn stated_entity_phrase(text: &str) -> Option<String> {
    let words = words(text);
    for index in 0..words.len() {
        if !ENTITY_PREPOSITIONS.contains(&words[index].to_lowercase().as_str()) {
            continue;
        }
        let mut end = index + 1;
        while end < words.len()
            && is_title_case(words[end])
            && month_number(&words[end].to_lowercase()).is_none()
        {
            end += 1;
        }
        if end > index + 1 {
            return Some(words[index + 1..end].join(" "));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 25).unwrap()
    }

    #[test]
    fn month_range_binds_stated_year_and_clamps_open_end_to_today() {
        let period =
            parse_period("Total deposits from January to September 2026.", today()).unwrap();
        assert_eq!(period.from, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        // September 2026 is the current month: the answer cannot include days
        // that have not happened yet.
        assert_eq!(period.to, today());
    }

    #[test]
    fn month_range_in_indonesian_matches_the_same_pattern() {
        let period = parse_period("Total setoran Januari sampai September 2026.", today()).unwrap();
        assert_eq!(period.from, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(period.to, today());
    }

    #[test]
    fn month_range_uses_end_month_year_when_only_one_year_is_given() {
        let period = parse_period("Deposits from October to December 2025.", today()).unwrap();
        assert_eq!(period.from, NaiveDate::from_ymd_opt(2025, 10, 1).unwrap());
        assert_eq!(period.to, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn year_only_binds_the_full_year_clamped_to_today() {
        let period = parse_period("Monthly office openings for 2026.", today()).unwrap();
        assert_eq!(period.from, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(period.to, today());

        let past_year = parse_period("Pembukaan kantor bulanan untuk 2025.", today()).unwrap();
        assert_eq!(past_year.from, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        assert_eq!(past_year.to, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn bare_year_without_context_word_is_never_guessed() {
        // "366" alone (e.g. a stray number) must not be read as a year.
        assert!(parse_period("366 days of transactions", today()).is_none());
    }

    #[test]
    fn relative_phrases_bind_today_and_this_month() {
        assert_eq!(
            parse_period("How much savings deposit did we receive today?", today()),
            Some(ParsedPeriod {
                from: today(),
                to: today(),
                detail: "today".into()
            })
        );
        assert_eq!(
            parse_period("Total setoran bulan ini.", today()),
            Some(ParsedPeriod {
                from: today().with_day(1).unwrap(),
                to: today(),
                detail: "this month".into()
            })
        );
    }

    #[test]
    fn last_n_months_covers_the_stated_span_ending_today() {
        // Exact-day subtraction, matching `business_today - 12m` (the same
        // span a capability's own manifest default already computes for this
        // phrasing) — never a calendar-month-aligned window.
        let period = parse_period("Top withdrawals in the last 12 months.", today()).unwrap();
        assert_eq!(period.from, NaiveDate::from_ymd_opt(2025, 9, 25).unwrap());
        assert_eq!(period.to, today());
    }

    #[test]
    fn no_recognizable_period_is_none() {
        assert!(parse_period("What is the total deposit this quarter?", today()).is_none());
    }

    #[test]
    fn top_n_and_indonesian_superlative_bind_the_stated_limit() {
        assert_eq!(
            parse_limit("Top 3 deposits per month"),
            Some(ParsedLimit {
                value: 3,
                detail: "top 3".into()
            })
        );
        assert_eq!(
            parse_limit("3 setoran terbesar per bulan"),
            Some(ParsedLimit {
                value: 3,
                detail: "3 terbesar".into()
            })
        );
        assert!(parse_limit("Total deposits this month").is_none());
    }

    #[test]
    fn currency_code_and_common_names_are_recognized() {
        assert_eq!(
            parse_currency("Show top 5 clients by savings balance in IDR"),
            Some(ParsedCurrency {
                code: "IDR".into(),
                detail: "IDR".into()
            })
        );
        assert_eq!(
            parse_currency("Total setoran dalam rupiah"),
            Some(ParsedCurrency {
                code: "IDR".into(),
                detail: "rupiah".into()
            })
        );
        assert!(parse_currency("Total deposits this month").is_none());
    }

    #[test]
    fn entity_phrase_matches_a_capitalized_name_after_a_preposition() {
        assert_eq!(
            stated_entity_phrase("Show me all clients from Head Office."),
            Some("Head Office".to_string())
        );
        assert_eq!(
            stated_entity_phrase("Tampilkan semua nasabah dari Kantor Pusat."),
            Some("Kantor Pusat".to_string())
        );
    }

    #[test]
    fn entity_phrase_never_matches_a_month_or_an_all_caps_currency_code() {
        assert_eq!(
            stated_entity_phrase("Total deposits from January to September 2026."),
            None
        );
        assert_eq!(
            stated_entity_phrase("Show top 5 clients by savings balance in IDR"),
            None
        );
    }
}
