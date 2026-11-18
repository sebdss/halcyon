//! Jeux offerts par l'Epic Games Store (1.1) : liste publique de la boutique, sans compte.
//! Seules les offres **en cours** à 100 % sont retenues.

use serde_json::Value;

pub fn url(locale: &str, country: &str) -> String {
    format!(
        "https://store-site-backend-static-ipv4.ak.epicgames.com/freeGamesPromotions?locale={}&country={}&allowCountries={}",
        crate::plugins::encode(locale),
        crate::plugins::encode(country),
        crate::plugins::encode(country)
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct FreeGame {
    pub id: String,
    pub title: String,
    /// Fin de l'offre (horodatage Unix).
    pub until: Option<i64>,
    pub url: String,
}

/// « 2026-11-19T16:00:00.000Z » → horodatage Unix.
pub fn parse_iso(s: &str) -> Option<i64> {
    let (date, time) = s.trim().split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.trim_end_matches('Z');
    let mut t = time.split(['.', '+']).next()?.split(':').map(|p| p.parse::<i64>().ok());
    let (h, mi, sec) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
    Some(halcyon_core::util::days_from_civil(y, m as u32, day as u32) * 86_400 + h * 3600 + mi * 60 + sec)
}

pub fn parse(v: &Value, locale: &str) -> Vec<FreeGame> {
    let lang = locale.split(['-', '_']).next().unwrap_or("en").to_lowercase();
    v.pointer("/data/Catalog/searchStore/elements")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|e| {
                    let offers = e.pointer("/promotions/promotionalOffers")?.as_array()?;
                    let offer = offers
                        .iter()
                        .filter_map(|g| g.get("promotionalOffers")?.as_array())
                        .flatten()
                        .find(|o| o.pointer("/discountSetting/discountPercentage").and_then(Value::as_i64) == Some(0))?;
                    if e.pointer("/price/totalPrice/discountPrice").and_then(Value::as_i64).is_some_and(|p| p != 0) {
                        return None;
                    }
                    let title = e.get("title")?.as_str()?.trim().to_string();
                    let slug = e
                        .pointer("/catalogNs/mappings/0/pageSlug")
                        .and_then(Value::as_str)
                        .or_else(|| e.get("productSlug").and_then(Value::as_str))
                        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
                    Some(FreeGame {
                        id: e.get("id").and_then(Value::as_str).unwrap_or(&title).to_string(),
                        until: offer.get("endDate").and_then(Value::as_str).and_then(parse_iso),
                        url: slug.map(|s| format!("https://store.epicgames.com/{lang}/p/{s}")).unwrap_or_else(|| format!("https://store.epicgames.com/{lang}/free-games")),
                        title,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_current_full_discounts() {
        let v = serde_json::json!({"data": {"Catalog": {"searchStore": {"elements": [
            {"id": "a", "title": "Knightica", "catalogNs": {"mappings": [{"pageSlug": "knightica-3be45c"}]},
             "price": {"totalPrice": {"discountPrice": 0}},
             "promotions": {"promotionalOffers": [{"promotionalOffers": [{"endDate": "2026-11-19T16:00:00.000Z", "discountSetting": {"discountPercentage": 0}}]}]}},
            {"id": "b", "title": "Promo", "price": {"totalPrice": {"discountPrice": 500}},
             "promotions": {"promotionalOffers": [{"promotionalOffers": [{"discountSetting": {"discountPercentage": 50}}]}]}},
            {"id": "c", "title": "Bientôt", "promotions": {"promotionalOffers": [], "upcomingPromotionalOffers": [{"promotionalOffers": [{"discountSetting": {"discountPercentage": 0}}]}]}},
            {"id": "d", "title": "Sans promo", "promotions": null}
        ]}}}});
        let g = parse(&v, "fr-FR");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].title, "Knightica");
        assert_eq!(g[0].url, "https://store.epicgames.com/fr/p/knightica-3be45c");
        assert_eq!(g[0].until, Some(halcyon_core::util::days_from_civil(2026, 11, 19) * 86_400 + 16 * 3600));
        assert_eq!(parse_iso("1970-01-02T00:00:01Z"), Some(86_401));
        assert_eq!(parse_iso("pas une date"), None);
    }
}
