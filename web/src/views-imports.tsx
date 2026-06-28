import * as api from "./api";
import { usePlayer } from "./player";
import type { ImportJob } from "./types";
import { ScreenHeader, SectionTitle, Status, useAsync } from "./views";

const ACTIVE_IMPORT_STATUSES = new Set(["resolving", "piping", "finalizing", "importing"]);

function importStatusLabel(status: string): string {
  switch (status) {
    case "resolving":
      return "Ищем источник";
    case "piping":
      return "Качаем";
    case "finalizing":
    case "importing":
      return "Импортируем";
    case "imported":
      return "Готово";
    case "needs_review":
      return "Нужно проверить";
    case "failed":
      return "Ошибка";
    default:
      return status;
  }
}

function importSection(jobs: ImportJob[], kind: "active" | "review" | "failed" | "imported") {
  switch (kind) {
    case "active":
      return jobs.filter((job) => ACTIVE_IMPORT_STATUSES.has(job.status));
    case "review":
      return jobs.filter((job) => job.status === "needs_review");
    case "failed":
      return jobs.filter((job) => job.status === "failed");
    case "imported":
      return jobs.filter((job) => job.status === "imported");
  }
}

function shortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("ru", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function ImportJobRow({ job }: { job: ImportJob }) {
  const { playQueue } = usePlayer();
  const playable = job.status === "imported" && Boolean(job.realSubsonicId);
  const content = (
    <>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-bold">{job.title}</span>
        <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400">
          {job.artist}
          {job.album ? ` · ${job.album}` : ""}
        </span>
        {job.error && (
          <span className="mt-1 block line-clamp-2 text-xs font-medium text-red-500 dark:text-red-300">
            {job.error}
          </span>
        )}
      </span>
      <span className="flex shrink-0 flex-col items-end gap-1 text-right">
        <span className="rounded-full bg-wave-pink/10 px-2 py-1 text-[10px] font-bold uppercase tracking-[0.14em] text-wave-pink">
          {job.provider}
        </span>
        <span className="text-[11px] font-semibold text-neutral-500 dark:text-neutral-400">
          {importStatusLabel(job.status)}
        </span>
        <span className="text-[11px] text-neutral-400">
          {job.matchScore !== undefined ? `${job.matchScore}% · ` : ""}
          {shortDate(job.updatedAt)}
        </span>
      </span>
    </>
  );

  if (playable) {
    return (
      <button
        type="button"
        onClick={() =>
          playQueue(
            [
              {
                id: job.realSubsonicId!,
                title: job.title,
                artist: job.artist,
                album: job.album,
                provider: job.provider,
              },
            ],
            0,
          )
        }
        className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-2 text-left transition hover:bg-black/[0.04] active:bg-black/[0.06] dark:hover:bg-white/[0.04] dark:active:bg-white/[0.07]"
      >
        {content}
      </button>
    );
  }

  return (
    <div className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-2">
      {content}
    </div>
  );
}

function ImportStatusSection({
  title,
  jobs,
}: {
  title: string;
  jobs: ImportJob[];
}) {
  if (jobs.length === 0) return null;
  return (
    <section className="mb-7">
      <SectionTitle>{title}</SectionTitle>
      <div className="rounded-xl border border-black/5 bg-white/40 p-3 backdrop-blur dark:border-white/10 dark:bg-white/[0.03]">
        {jobs.map((job) => (
          <ImportJobRow key={job.id} job={job} />
        ))}
      </div>
    </section>
  );
}

export function ImportsView() {
  const { session } = usePlayer();
  const data = useAsync(() => api.getImports(session), [session]);
  const jobs = data.data ?? [];
  const active = importSection(jobs, "active");
  const review = importSection(jobs, "review");
  const failed = importSection(jobs, "failed");
  const imported = importSection(jobs, "imported");
  const empty =
    !data.loading &&
    !data.error &&
    active.length === 0 &&
    review.length === 0 &&
    failed.length === 0 &&
    imported.length === 0;

  return (
    <div className="animate-fade-in">
      <ScreenHeader title="Импорт" />
      <Status loading={data.loading} error={data.error} />
      {empty && (
        <div className="rounded-xl border border-wave-pink/15 bg-wave-pink/5 px-5 py-8 text-center">
          <p className="font-bold">Пока нет импортов.</p>
          <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">
            Они появятся здесь после проигрывания внешних треков.
          </p>
        </div>
      )}
      <ImportStatusSection title="Активные" jobs={active} />
      <ImportStatusSection title="Нужно проверить" jobs={review} />
      <ImportStatusSection title="Ошибки" jobs={failed} />
      <ImportStatusSection title="Готово" jobs={imported} />
    </div>
  );
}

