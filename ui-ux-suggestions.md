# Songarr Wave — UI/UX suggestions

Basis: logged into the running app (onboarding live), read the JSX/Tailwind for the
inner screens (backend proxy/Navidrome weren't running). Ranked by leverage —
what most causes the "fine but not amazing" feeling.

## The three things actually holding it back

### 1. The signature gradient is monochrome
`web/src/index.css` tokens are all the same hue, a few % of lightness apart:

- `wave-orange: #a4243b` (crimson)
- `wave-pink:   #c41e3a` (crimson)
- `wave-violet: #45122e` (dark maroon)

So every "gradient" (hero, primary buttons, logo tile, CTAs) renders as a flat red
smear with no depth. The wine/goth direction is good and distinctive (Cormorant
serif, gothic cross, `gothic-rule` dividers) — the fix is just to **spread the
stops**: keep the wine mid, push one toward a brighter ember and one toward a
near-black plum. Same identity, far richer gradient. Highest-impact change.

### 2. Two of every control idiom — unify each
- **Section headers:** Home uses big `text-lg font-bold` + pink "Всё" link; other
  screens use the tiny uppercase tracked `SectionTitle` with the gothic rule.
  Standardize on `SectionTitle`.
- **"Pick one" controls:** album filter = white-pill-active segmented control;
  quality selector = pink-tint bordered buttons. Same job, two looks.
- **Radius:** album segmented control is `rounded-lg` containing `rounded-xl`
  children (corners poke out). Define a scale: covers/cards one value, controls
  one value, pills full.
- **Primary buttons:** mix of 3-stop gradient, 2-stop gradient, and solid neutral.
  Collapse to one after the palette fix.

### 3. Loading is a lone spinner; empty states barely exist
- Every list fetch shows one centered spinner, then pops in. Use **skeleton tiles**
  (greyed cover squares) for the grid-shaped screens.
- Empty states are uneven: Liked has a nice one (icon-in-circle + copy); Playlists,
  Albums, Artist render nothing when empty. Add one shared `<EmptyState>`.

## Smaller papercuts
- **Raw error text:** login shows literal "Server returned HTTP 500". Map to human
  copy.
- **List noise:** `SongRow` shows download + like on every row always; reveal on
  hover for desktop, keep always-on for touch.
- **Underused identity:** Cormorant only appears in ~3 spots; lean it into section
  dividers / numbers.
- **Dead weight:** app is hardcoded dark (`<html class="dark">`), so all
  `bg-white/… dark:bg-white/…` light pairings never render (~50 dead classes).

## Plan / status
Starting with #1 (palette) + #3 (skeletons + shared empty state) — biggest perceived
lift for least work. #2 is a broader consistency pass to follow.

- [x] #1 Palette: spread the three gradient stops in `index.css`
      (orange `#a4243b`→`#e8452a` ember, violet `#45122e`→`#34093a` plum; pink
      accent unchanged). Verified live on the login gradient.
- [x] #3a Shared `<EmptyState>` + `<SkeletonCardGrid>`/`<SkeletonRows>`/`<Skeleton>`
      in `components.tsx`
- [x] #3b Skeletons wired into Albums / Artist / Library(artists) / Liked / Playlists
      / Search (Home carousels left as-is — bespoke layout)
- [x] #3c Empty states for Playlists / Albums / Artist / Search (initial + no-results);
      Liked switched to the shared component
- [x] i18n: added `playlistsEmpty / albumsEmpty / artistNoAlbums / searchPrompt /
      searchEmpty` in en/de/ru (parity test passes)
- [x] #2 Consistency pass:
      - One shared `<Segmented>` (components.tsx) replaces 5 copies of "pick one"
        controls: onboarding quality, settings quality (stream+download), settings
        language, and the Albums sort filter. The album filter's odd white-pill
        segmented track (and its rounded-lg/rounded-xl poke-out) is gone — it now
        uses the same pink-tint chip as everything else.
      - Canonical `STREAM_QUALITY_CHOICES` / `DOWNLOAD_QUALITY_CHOICES` moved to
        quality.ts (was duplicated in App.tsx + views-settings.tsx).
      - `SectionTitle` gained an optional `action` slot; Home's two bespoke
        big-bold headers now use it, so every category header in the app is the
        same gothic-rule style. No-action call sites are visually unchanged.
      - Primary CTA gradient unified on the 3-stop signature (ember→crimson→plum):
        upgraded PlayAll, Save, and Shuffle-downloads from the flat 2-stop. Thin
        progress/seek bars stay 2-stop intentionally.
- [ ] Papercuts: humanize raw error text; hover-reveal row actions on desktop
