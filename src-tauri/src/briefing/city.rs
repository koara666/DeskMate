//! 城市：内置常用城市表（中文名 → 坐标、时区，`data/cities.tsv`），查不到再用 Open-Meteo 地理编码（只认英文 / 拼音）。
//! 用户选的城市只存在本地设置里（DECISIONS #39）。

use serde::{Deserialize, Serialize};

const TABLE: &str = include_str!("../../data/cities.tsv");

/// 用户选定的城市
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct City {
    /// 显示名（中文）
    pub name: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub tz: String,
}

impl City {
    pub fn valid(&self) -> bool {
        self.lat.is_finite()
            && self.lon.is_finite()
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
            && !self.name.trim().is_empty()
            && self.name.chars().count() <= 40
            && self.country.len() <= 3
    }
}

struct Row {
    zh: &'static str,
    aliases: Vec<&'static str>,
    en: &'static str,
    city: City,
}

fn rows() -> &'static [Row] {
    static ROWS: std::sync::OnceLock<Vec<Row>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        TABLE
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let f: Vec<&'static str> = l.split('\t').collect();
                if f.len() < 7 {
                    return None;
                }
                Some(Row {
                    zh: f[0],
                    aliases: f[1].split('|').filter(|x| !x.is_empty()).collect(),
                    en: f[2],
                    city: City {
                        name: f[0].into(),
                        country: f[3].into(),
                        lat: f[4].parse().ok()?,
                        lon: f[5].parse().ok()?,
                        tz: f[6].into(),
                    },
                })
            })
            .collect()
    })
}

fn strip_suffix(q: &str) -> &str {
    q.trim()
        .trim_end_matches(['市', '都', '府', '县', '縣'])
        .trim()
}

/// 在内置表里找（中文名、别名、英文名，开头匹配）
pub fn search_local(q: &str) -> Vec<City> {
    let q = strip_suffix(q);
    if q.is_empty() {
        return Vec::new();
    }
    let ql = q.to_lowercase();
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    for r in rows() {
        let names = std::iter::once(r.zh).chain(r.aliases.iter().copied());
        let is_exact = names.clone().any(|n| n == q) || r.en.to_lowercase() == ql;
        let is_prefix = names.clone().any(|n| n.starts_with(q))
            || (ql.len() >= 2 && r.en.to_lowercase().starts_with(&ql));
        if is_exact {
            exact.push(r.city.clone());
        } else if is_prefix {
            prefix.push(r.city.clone());
        }
    }
    exact.extend(prefix);
    exact.truncate(10);
    exact
}

/// 一句话里提到的城市（聊天时问「大阪明天天气怎么样」）：只认内置表里两个字以上的中文名和别名，取最长的
pub fn mentioned(text: &str) -> Option<City> {
    rows()
        .iter()
        .flat_map(|r| {
            std::iter::once(r.zh)
                .chain(r.aliases.iter().copied())
                .map(move |n| (n, r))
        })
        .filter(|(n, _)| n.chars().count() >= 2 && text.contains(n))
        .max_by_key(|(n, _)| n.chars().count())
        .map(|(_, r)| r.city.clone())
}

#[derive(Deserialize)]
struct GeoResp {
    #[serde(default)]
    results: Vec<GeoItem>,
}

#[derive(Deserialize)]
struct GeoItem {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country_code: String,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    timezone: String,
}

/// 用 Open-Meteo 地理编码查（用户在设置里点「查找」时才联网；只认英文 / 拼音 / 罗马字）
pub async fn search_online(http: &reqwest::Client, q: &str) -> Result<Vec<City>, String> {
    let q = q.trim();
    if q.chars().count() < 2 || q.chars().count() > 60 {
        return Ok(Vec::new());
    }
    let url = reqwest::Url::parse_with_params(
        "https://geocoding-api.open-meteo.com/v1/search",
        &[
            ("name", q),
            ("count", "8"),
            ("language", "zh"),
            ("format", "json"),
        ],
    )
    .map_err(|e| e.to_string())?;
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| {
            format!(
                "查不到：网络不通（{}）",
                crate::redact::redact_secrets(&e.to_string())
            )
        })?;
    if !resp.status().is_success() {
        return Err(format!("查不到：服务器返回 {}", resp.status().as_u16()));
    }
    let body: GeoResp = resp
        .json()
        .await
        .map_err(|_| "查不到：返回的数据看不懂".to_string())?;
    Ok(body
        .results
        .into_iter()
        .map(|g| City {
            name: match g.admin1 {
                Some(a) if a != g.name => format!("{}（{a}）", g.name),
                _ => g.name,
            }
            .chars()
            .take(40)
            .collect(),
            country: g.country_code.chars().take(3).collect(),
            lat: g.latitude,
            lon: g.longitude,
            tz: g.timezone,
        })
        .filter(City::valid)
        .collect())
}

/// 聊天里提到的地名（内置表里没有的）：用地理编码查坐标。两个字的中文地名只认带「市」的写法，所以先试「抚顺市」再试「抚顺」。
/// 查不到 / 网络不通返回 None（调用方再退回联网搜索）
pub async fn resolve_online(http: &reqwest::Client, place: &str) -> Option<City> {
    let place = place.trim();
    let mut tries = Vec::new();
    if place.chars().all(|c| !c.is_ascii()) && !place.ends_with(['市', '县', '区', '縣']) {
        tries.push(format!("{place}市"));
    }
    tries.push(place.to_string());
    for q in tries {
        if let Ok(found) = search_online(http, &q).await
            && let Some(c) = found.into_iter().next()
        {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_and_searches() {
        assert!(rows().len() >= 100);
        assert!(rows().iter().all(|r| r.city.valid()));
        let osaka = search_local("大阪");
        assert_eq!(osaka[0].name, "大阪");
        assert_eq!(osaka[0].country, "JP");
        assert!((osaka[0].lat - 34.69).abs() < 0.1);
        assert_eq!(search_local("大阪市")[0].name, "大阪", "去掉「市」");
        assert_eq!(search_local("東京")[0].name, "东京", "日文写法");
        assert_eq!(search_local("osaka")[0].name, "大阪");
        assert_eq!(search_local("上海")[0].tz, "Asia/Shanghai");
        assert!(search_local("火星").is_empty());
        // 开头匹配：「哈」→ 哈尔滨
        assert!(search_local("哈").iter().any(|c| c.name == "哈尔滨"));
    }

    #[test]
    fn finds_city_mentioned_in_chat() {
        assert_eq!(mentioned("大阪明天的天气怎么样").unwrap().name, "大阪");
        assert_eq!(mentioned("胡志明市热吗").unwrap().name, "胡志明市");
        assert!(mentioned("明天会下雨吗").is_none());
    }

    #[test]
    fn city_validation() {
        let mut c = search_local("北京").remove(0);
        assert!(c.valid());
        c.lat = 200.0;
        assert!(!c.valid());
    }

    /// 真的联网：中文地名（表里没有的）能查到坐标。`cargo test -- --ignored resolve_live`
    #[tokio::test]
    #[ignore = "联网"]
    async fn resolve_live() {
        let http = reqwest::Client::new();
        let c = resolve_online(&http, "抚顺").await.expect("查到抚顺");
        assert_eq!(c.country, "CN");
        assert!((c.lat - 41.9).abs() < 0.5 && (c.lon - 123.9).abs() < 0.5);
        assert!(resolve_online(&http, "不存在的地方啦啦啦").await.is_none());
    }
}
