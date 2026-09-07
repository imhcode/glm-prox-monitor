# Provider: glmprox (GLM Proxy)

Provider default glm-overflow. Data usage diambil dari instance
[glm-prox-monitor](https://github.com/imhcode/glm-prox-monitor) — proxy GLM coding
plan yang mengekspos satu endpoint statistik.

## Metrik yang ditampilkan

- Progress window **5 jam**: % sisa, sisa token, countdown reset
- Panel detail: nama, model, status langganan, window mulai/berakhir,
  total request/token, terakhir dipakai, expiry langganan, key

## Kredensial

| Sumber | Keterangan |
| --- | --- |
| Settings app | **API Token** + **Base URL** (tersimpan di `config.json` lokal) |
| Env `GLM_OVERFLOW_TOKEN` | Token default saat first-run (config menang bila sudah diisi) |

Base URL default: `https://glm.ajianaz.dev`.

## Endpoint

```
GET {base_url}/stats
Authorization: Bearer <token>
```

Field utama yang dipakai: `token_limit_per_5h`,
`current_usage.tokens_used_in_current_window`, `current_usage.remaining_tokens`,
`current_usage.window_ends_at`, `expiry_date`, `last_used`, `is_expired`,
`total_requests`, `total_lifetime_tokens`.

## State error

| Pesan | Arti |
| --- | --- |
| `limited` + countdown | Kena rate limit window 5 jam — reset otomatis sesuai `error.window_ends_at` |
| `HTTP <status>` | Server proxy menolak/bermasalah — cek instance glmprox |
| `jaringan: …` | Koneksi gagal — cek internet atau base URL |
| `parse: …` | Respons tidak sesuai skema — kemungkinan versi glmprox tidak kompatibel |
