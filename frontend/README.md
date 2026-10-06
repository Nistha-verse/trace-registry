# Trace frontend

The frontend is a Vite + React read-only verifier for the Trace API.

```bash
npm install
cp .env.example .env.local
npm run dev
```

Production checks:

```bash
npm test
npm run build
```

Configure `VITE_TRACE_API_URL` for the verifier service. Cluster-specific API
URLs can be supplied through `VITE_TRACE_API_URL_MAINNET`,
`VITE_TRACE_API_URL_DEVNET`, and `VITE_TRACE_API_URL_LOCALNET`.
