// Wave PWA shell (scaffold). The hero button and now-playing bar are the
// real W1 surfaces; wiring to /wave/api/next + <audio> lands in W1–W2 per
// songarr-wave-plan.md. For now this renders the screen so `npm run dev`
// (proxying /rest + /wave/api to the dev songarr) shows the target layout.

function PlayIcon({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className}>
      <path d="M8 5v14l11-7z" />
    </svg>
  );
}

export default function App() {
  return (
    <div className="min-h-full bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-50">
      <div className="mx-auto flex min-h-full max-w-md flex-col px-5 pb-28 pt-6">
        <header className="mb-6 flex items-center gap-3">
          <div className="grid h-9 w-9 place-items-center rounded-full bg-neutral-200 dark:bg-neutral-800">
            <PlayIcon className="h-4 w-4 text-amber-500" />
          </div>
          <h1 className="text-2xl font-bold tracking-tight">Музыка</h1>
        </header>

        {/* Hero — the one button that matters */}
        <button
          type="button"
          className="group relative mb-8 aspect-[16/10] w-full overflow-hidden rounded-3xl text-left shadow-lg"
          style={{
            background:
              "linear-gradient(120deg,#ff8a3d 0%,#ff4d9d 45%,#a23bff 100%)",
          }}
        >
          <span className="absolute right-5 top-5 grid h-14 w-14 place-items-center rounded-full bg-white text-purple-700 shadow-md transition group-active:scale-95">
            <PlayIcon className="ml-0.5 h-7 w-7" />
          </span>
          <span className="absolute inset-x-5 bottom-5 block">
            <span className="block text-3xl font-extrabold text-white drop-shadow">
              Твоя волна
            </span>
            <span className="mt-1 block max-w-[80%] text-sm font-medium text-white/85">
              бесконечный поток музыки, подобранной для тебя
            </span>
          </span>
        </button>

        <section>
          <h2 className="mb-3 text-lg font-bold">Подборки</h2>
          <div className="flex gap-3 overflow-x-auto pb-2">
            {["Классика рока и метала", "Французский шансон", "Поп-волна"].map(
              (name) => (
                <div key={name} className="w-32 shrink-0">
                  <div className="aspect-square w-full rounded-2xl bg-gradient-to-br from-indigo-400 to-fuchsia-500" />
                  <p className="mt-2 text-sm font-semibold leading-tight">
                    {name}
                  </p>
                  <p className="text-xs text-neutral-500">подборка</p>
                </div>
              ),
            )}
          </div>
        </section>
      </div>

      {/* Now-playing bar */}
      <div className="fixed inset-x-0 bottom-0 border-t border-neutral-200 bg-white/90 backdrop-blur dark:border-neutral-800 dark:bg-neutral-900/90">
        <div className="mx-auto flex max-w-md items-center gap-3 px-5 py-3">
          <div className="h-10 w-10 shrink-0 rounded-lg bg-neutral-200 dark:bg-neutral-700" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">Нажми «Твоя волна»</p>
            <p className="truncate text-xs text-neutral-500">
              ничего не играет
            </p>
          </div>
          <button
            type="button"
            aria-label="play"
            className="grid h-9 w-9 place-items-center rounded-full bg-neutral-900 text-white dark:bg-white dark:text-neutral-900"
          >
            <PlayIcon className="ml-0.5 h-5 w-5" />
          </button>
        </div>
      </div>
    </div>
  );
}
