# Provider: Z.ai

Provider Z.ai langsung — Z.ai adalah vendor pemilik **GLM Coding Plan**, jadi
usage bisa dipantau tanpa proxy. Integrasi mengikuti pola
[openusage](https://github.com/robinebers/openusage) (docs/providers/zai.md).

## Metrik yang ditampilkan

| Meter | Arti |
| --- | --- |
| **Sesi (5 jam)** | Window token berjalan (persentase) — jadi indikator utama pill |
| **Mingguan** | Window 7 hari (persentase) |
| **Bulanan** | Window 30 hari (persentase), bila dilaporkan |
| **Web Search** | Jumlah panggilan web-search bulanan (terpakai / limit) |

## Kredensial

API key diambil dari console Z.ai (z.ai → **API Keys**) — butuh langganan
GLM Coding aktif. Urutan pencarian (yang atas menang):

1. Settings app → **API Key Z.ai** (tersimpan di `config.json` lokal)
2. Env `ZAI_API_KEY`

Key bisa dirotasi kapan saja lewat Settings tanpa sentuh file manual.

## Endpoint

Endpoint internal yang sama dengan UI langganan Z.ai (tidak resmi, stabil di
praktik; implementasi mengikuti mapper openusage):

```
GET https://api.z.ai/api/monitor/usage/quota/limit   -> meter kuota (wajib)
GET https://api.z.ai/api/biz/subscription/list       -> nama plan (best-effort)
Authorization: Bearer <API key>
```

Bentuk respons kuota: `data.limits[]` dengan entri `CREDIT_LIMIT`
(field `unit`, `number`, `percentage`, `nextResetTime` epoch ms) dan `TIME_LIMIT`
(`currentValue` = terpakai, `usage` = limit). Panjang window dari
`unit × number`: < 1 hari → meter sesi, selainnya mingguan/bulanan.
Bila Z.ai mengubah bentuk respons, hanya `src-tauri/src/providers/zai.rs`
yang perlu disesuaikan.

## State error

| Pesan | Arti |
| --- | --- |
| API key Z.ai belum diisi | Tidak ada key di Settings / env `ZAI_API_KEY` |
| API key Z.ai ditolak (HTTP 401/403) | Key invalid/dicabut — regenerate di console |
| Tidak ada paket GLM Coding aktif | Key valid tapi akun tanpa paket meterable — berlangganan di z.ai/subscribe |
| Belum ada data usage | Paket ada tapi kuota belum tersedia — coba refresh nanti |
| Respons kuota tidak dikenal | Z.ai mengubah format — tunggu pembaruan app |
