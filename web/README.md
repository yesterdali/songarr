# Songarr Wave (Твоя волна)

React + Tailwind PWA for the one-button endless personalized radio. See
`../songarr-wave-plan.md` for the full plan and milestones.

## Dev

```sh
npm install
npm run dev        # http://localhost:5173/wave/
```

Vite proxies `/rest` and `/wave/api` to the dev songarr at
`http://127.0.0.1:4534` (override with `SONGARR_URL=…`). Keep that proxy
running so the PWA can authenticate and stream.

## Build

```sh
npm run build      # → web/dist, later embedded into the songarr binary (W5)
```

## Status

Scaffold (pre-W1): renders the target screen. Auth, the wave queue, playback,
feedback, and PWA install behavior land per the milestones in the plan.
