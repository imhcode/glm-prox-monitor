//! Provider Z.ai langsung — vendor pemilik GLM Coding Plan, tanpa proxy.
//! Endpoint internal yang sama dengan yang dipakai openusage
//! (docs/providers/zai.md):
//!   GET /api/biz/subscription/list     -> nama plan (best-effort)
//!   GET /api/monitor/usage/quota/limit -> meter sesi/mingguan/web search
//! Endpoint tidak resmi (stabil di praktik) — mapper dibuat defensif;
//! bila Z.ai mengubah bentuk respons, cukup file ini yang disesuaikan.

use serde_json::Value;

use super::{Meter, ProviderKind, Snapshot};
use crate::config::Config;

const BASE_URL: &str = "https://api.z.ai";
const SUBSCRIPTION_PATH: &str = "/api/biz/subscription/list";
const QUOTA_PATH: &str = "/api/monitor/usage/quota/limit";

/// Satu ms dalam sehari — batas pemisah window sesi (< 1 hari) vs mingguan.
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

pub async fn fetch(cfg: &Config) -> Snapshot {
    let info = ProviderKind::Zai.info();

    let key = effective_key(cfg);
    if key.is_empty() {
        return Snapshot::failed(
            info,
            "API key Z.ai belum diisi — isi di Settings atau set env ZAI_API_KEY.".into(),
        );
    }

    let client = match super::http_client() {
        Ok(c) => c,
        Err(e) => return Snapshot::failed(info, e),
    };

    // Kuota = sumber utama snapshot; gagal di sini berarti snapshot gagal.
    let resp = match client
        .get(format!("{BASE_URL}{QUOTA_PATH}"))
        .bearer_auth(&key)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Snapshot::failed(info, format!("jaringan: {e}")),
    };

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Snapshot::failed(
            info,
            format!("API key Z.ai ditolak (HTTP {status}) — periksa key di console Z.ai."),
        );
    }

    let text = resp.text().await.unwrap_or_default();
    let root: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return Snapshot::failed(info, format!("parse: {e}")),
    };

    let meters = match map_quota_response(&root) {
        Ok(m) => m,
        Err(QuotaError::NoPlan) => {
            return Snapshot::failed(
                info,
                "Tidak ada paket GLM Coding aktif di akun Z.ai ini.".into(),
            )
        }
        Err(QuotaError::NoUsageData) => {
            return Snapshot::failed(
                info,
                "Belum ada data usage — key valid tapi kuota belum tersedia.".into(),
            )
        }
        Err(QuotaError::Invalid) => {
            return Snapshot::failed(info, format!("Respons kuota tidak dikenal (HTTP {status})."))
        }
    };

    // Nama plan: best-effort — kegagalan tidak boleh menghapus meter.
    let plan = fetch_plan(&client, &key).await;

    let mut snap = Snapshot::new(info, "ok");
    snap.plan = plan;
    snap.meters = meters;
    snap
}

/// Precedensi ala openusage: key tersimpan di config menang atas env.
fn effective_key(cfg: &Config) -> String {
    let saved = cfg.zai_api_key.trim();
    if !saved.is_empty() {
        return saved.to_string();
    }
    std::env::var("ZAI_API_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or_default()
}

async fn fetch_plan(client: &reqwest::Client, key: &str) -> Option<String> {
    let resp = client
        .get(format!("{BASE_URL}{SUBSCRIPTION_PATH}"))
        .bearer_auth(key)
        .send()
        .await
        .ok()?;
    let root: Value = resp.json().await.ok()?;
    extract_plan(&root)
}

/// Nama plan dari respons subscription/list: `data[0].productName`.
fn extract_plan(root: &Value) -> Option<String> {
    root.get("data")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|e| e.get("productName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[derive(Debug)]
enum QuotaError {
    /// success:false + pesan "coding plan" — key valid, tanpa paket meterable.
    NoPlan,
    /// Key valid, paket ada, tapi array limits kosong.
    NoUsageData,
    /// Bentuk respons tidak dikenali.
    Invalid,
}

/// Map respons kuota -> daftar meter terurut (sesi, mingguan/bulanan, web search).
fn map_quota_response(root: &Value) -> Result<Vec<Meter>, QuotaError> {
    if root.get("success").and_then(Value::as_bool) == Some(false) {
        let msg = root
            .get("msg")
            .or_else(|| root.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        return if msg.to_lowercase().contains("coding plan") {
            Err(QuotaError::NoPlan)
        } else {
            Err(QuotaError::Invalid)
        };
    }

    // Array limits hidup di data.limits; sebagian respons membungkus langsung di root.
    let container = match root.get("data") {
        Some(d) if d.is_object() => d,
        _ => root,
    };
    let Some(limits) = container.get("limits").and_then(Value::as_array) else {
        return Err(QuotaError::Invalid);
    };
    if limits.is_empty() {
        return Err(QuotaError::NoUsageData);
    }

    let mut meters: Vec<(u8, Meter)> = limits.iter().filter_map(map_limit_entry).collect();
    if meters.is_empty() {
        return Err(QuotaError::Invalid);
    }
    meters.sort_by_key(|(order, _)| *order);
    Ok(meters.into_iter().map(|(_, m)| m).collect())
}

/// Kode `unit` Z.ai -> panjang window ms (padanan ZAIUsageMapper openusage).
fn unit_ms(unit: i64) -> Option<u64> {
    match unit {
        3 => Some(3_600_000),        // jam
        4 => Some(DAY_MS),           // hari
        6 => Some(7 * DAY_MS),       // minggu
        5 => Some(30 * DAY_MS),      // bulan
        _ => None,
    }
}

fn entry_type(entry: &Value) -> &str {
    entry
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| entry.get("name").and_then(Value::as_str))
        .unwrap_or_default()
}

/// Angka numerik toleran — terima JSON number maupun string digit.
fn num(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn epoch_ms(v: Option<&Value>) -> Option<u64> {
    v.and_then(Value::as_u64)
        .or_else(|| v.and_then(Value::as_i64).filter(|n| *n > 0).map(|n| n as u64))
}

/// Map satu entri `limits[]` -> (urutan tampil, Meter). None = entry dilewati.
fn map_limit_entry(entry: &Value) -> Option<(u8, Meter)> {
    let resets = epoch_ms(entry.get("nextResetTime"));

    match entry_type(entry) {
        // Meter persentase — window: < 1 hari = sesi, selainnya mingguan/bulanan.
        "CREDIT_LIMIT" | "TOKENS_LIMIT" => {
            let unit = unit_ms(num(entry.get("unit"))? as i64)?;
            let number = num(entry.get("number")).filter(|n| *n > 0.0)?;
            let period = (unit as f64 * number) as u64;
            let pct = num(entry.get("percentage"))?.clamp(0.0, 100.0);

            let (order, label) = if period < DAY_MS {
                let hours = (period / 3_600_000).max(1);
                (0u8, format!("Sesi ({hours} jam)"))
            } else if period >= 28 * DAY_MS {
                (1, "Bulanan".to_string())
            } else {
                (1, "Mingguan".to_string())
            };

            Some((
                order,
                Meter {
                    label,
                    used_percent: pct,
                    used: None,
                    limit: None,
                    unit: "percent".into(),
                    resets_at_ms: resets,
                    period_ms: Some(period),
                },
            ))
        }
        // Web search bulanan — catatan openusage: nilai terbalik,
        // `currentValue` = terpakai, `usage` = limit.
        "TIME_LIMIT" => {
            let used = num(entry.get("currentValue"))?;
            let limit = num(entry.get("usage"))?;
            if used < 0.0 || limit < 0.0 {
                return None;
            }
            let pct = if limit > 0.0 {
                (used / limit * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };

            Some((
                2,
                Meter {
                    label: "Web Search".into(),
                    used_percent: pct,
                    used: Some(used),
                    limit: Some(limit),
                    unit: "count".into(),
                    resets_at_ms: resets,
                    period_ms: Some(30 * DAY_MS),
                },
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_meter_from_5h_credit_limit() {
        let root = json!({
            "success": true,
            "data": { "limits": [
                { "type": "CREDIT_LIMIT", "unit": 3, "number": 5,
                  "percentage": 41.5, "nextResetTime": 1757200000000u64 }
            ]}
        });
        let meters = map_quota_response(&root).unwrap();
        assert_eq!(meters.len(), 1);
        assert_eq!(meters[0].label, "Sesi (5 jam)");
        assert!((meters[0].used_percent - 41.5).abs() < 1e-9);
        assert_eq!(meters[0].resets_at_ms, Some(1_757_200_000_000));
        assert_eq!(meters[0].unit, "percent");
        assert_eq!(meters[0].period_ms, Some(5 * 3_600_000));
    }

    #[test]
    fn weekly_and_monthly_labels() {
        let weekly = json!({ "type": "CREDIT_LIMIT", "unit": 4, "number": 7, "percentage": 10 });
        let (order, m) = map_limit_entry(&weekly).unwrap();
        assert_eq!((order, m.label.as_str()), (1, "Mingguan"));

        let monthly = json!({ "type": "CREDIT_LIMIT", "unit": 5, "number": 1, "percentage": 10 });
        let (_, m) = map_limit_entry(&monthly).unwrap();
        assert_eq!(m.label, "Bulanan");
    }

    #[test]
    fn time_limit_reads_swapped_fields() {
        let entry = json!({ "type": "TIME_LIMIT", "currentValue": 12, "usage": 300,
                            "nextResetTime": 1759800000000u64 });
        let (order, m) = map_limit_entry(&entry).unwrap();
        assert_eq!(order, 2);
        assert_eq!(m.label, "Web Search");
        assert_eq!(m.unit, "count");
        assert_eq!(m.used, Some(12.0));
        assert_eq!(m.limit, Some(300.0));
        assert!((m.used_percent - 4.0).abs() < 1e-9);
    }

    #[test]
    fn legacy_tokens_limit_treated_as_credit() {
        let entry = json!({ "type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 7 });
        let (_, m) = map_limit_entry(&entry).unwrap();
        assert_eq!(m.label, "Sesi (5 jam)");
    }

    #[test]
    fn meters_sorted_session_first() {
        let root = json!({
            "success": true,
            "data": { "limits": [
                { "type": "TIME_LIMIT", "currentValue": 1, "usage": 100 },
                { "type": "CREDIT_LIMIT", "unit": 4, "number": 7, "percentage": 20 },
                { "type": "CREDIT_LIMIT", "unit": 3, "number": 5, "percentage": 10 }
            ]}
        });
        let meters = map_quota_response(&root).unwrap();
        let labels: Vec<&str> = meters.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, vec!["Sesi (5 jam)", "Mingguan", "Web Search"]);
    }

    #[test]
    fn unknown_unit_entry_skipped() {
        let entry = json!({ "type": "CREDIT_LIMIT", "unit": 99, "number": 5, "percentage": 1 });
        assert!(map_limit_entry(&entry).is_none());
        let root = json!({ "success": true, "data": { "limits": [entry] } });
        assert!(matches!(map_quota_response(&root), Err(QuotaError::Invalid)));
    }

    #[test]
    fn success_false_with_coding_plan_msg_is_no_plan() {
        let root = json!({ "success": false, "msg": "no coding plan found" });
        assert!(matches!(map_quota_response(&root), Err(QuotaError::NoPlan)));
    }

    #[test]
    fn success_false_with_other_msg_is_invalid() {
        let root = json!({ "success": false, "msg": "forbidden" });
        assert!(matches!(map_quota_response(&root), Err(QuotaError::Invalid)));
    }

    #[test]
    fn empty_limits_is_no_usage_and_missing_is_invalid() {
        let empty = json!({ "success": true, "data": { "limits": [] } });
        assert!(matches!(map_quota_response(&empty), Err(QuotaError::NoUsageData)));

        let missing = json!({ "success": true, "data": {} });
        assert!(matches!(map_quota_response(&missing), Err(QuotaError::Invalid)));
    }

    #[test]
    fn percentage_clamped_and_string_numbers_accepted() {
        let entry = json!({ "type": "CREDIT_LIMIT", "unit": "3", "number": "5",
                            "percentage": 140 });
        let (_, m) = map_limit_entry(&entry).unwrap();
        assert!((m.used_percent - 100.0).abs() < 1e-9);
    }

    #[test]
    fn plan_extracted_from_subscription() {
        let root = json!({ "success": true,
                           "data": [ { "productName": "GLM Coding Plan" } ] });
        assert_eq!(extract_plan(&root).as_deref(), Some("GLM Coding Plan"));

        let empty_name = json!({ "success": true, "data": [ { "productName": "" } ] });
        assert_eq!(extract_plan(&empty_name), None);

        assert_eq!(extract_plan(&json!({ "data": [] })), None);
    }
}
