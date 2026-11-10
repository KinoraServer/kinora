import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useParams, useSearchParams } from "react-router";
import { ItemGrid } from "@/components/ItemTile";
import { LetterRail } from "@/components/LetterRail";
import { Empty, ErrorNote, Spinner } from "@/components/ui/card";
import { Input, Select } from "@/components/ui/input";
import { useApi } from "@/lib/session";
import type { ItemCard, Library } from "@/lib/types";
import { cn } from "@/lib/utils";

export function LibraryPage() {
  const { t } = useTranslation();
  const { id } = useParams();
  const [sort, setSort] = useState("title");
  const [filter, setFilter] = useState("");
  const { data: libraries } = useApi<Library[]>("api/libraries");
  const library = libraries?.find((l) => l.id === id);
  const { data, error, loading } = useApi<ItemCard[]>(`api/items?library_id=${id}&sort=${sort}`, [id, sort]);
  const items = useMemo(
    () => data?.filter((i) => i.title.toLowerCase().includes(filter.toLowerCase())) ?? [],
    [data, filter],
  );
  const titles = useMemo(() => items.map((i) => i.title), [items]);
  const grid = useRef<HTMLDivElement>(null);
  // The A–Z rail only makes sense when the grid is in title order.
  const showRail = sort === "title" && items.length > 0;

  return (
    // Room on the right for the rail, so it never sits on top of a poster.
    <div className={cn("space-y-6", showRail && "pr-5 sm:pr-4")}>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">{library?.name ?? t("library.title")}</h1>
          {data && <p className="text-sm text-muted-foreground">{t("library.count", { count: data.length })}</p>}
        </div>
        <div className="flex gap-2">
          <Input
            placeholder={t("library.filter")}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="w-44"
            aria-label={t("library.filter")}
          />
          <Select
            value={sort}
            onChange={(e) => setSort(e.target.value)}
            className="w-40"
            aria-label={t("library.sort")}
          >
            <option value="title">{t("library.sort_title")}</option>
            <option value="added">{t("library.sort_added")}</option>
            <option value="year">{t("library.sort_year")}</option>
            <option value="rating">{t("library.sort_rating")}</option>
          </Select>
        </div>
      </div>
      {loading && !data && <Spinner className="mx-auto mt-16" />}
      <ErrorNote message={error} />
      {data && items.length === 0 && (
        <Empty title={data.length ? t("library.no_matches") : t("library.empty")}>
          {data.length ? t("library.no_matches_hint") : t("library.empty_hint")}
        </Empty>
      )}
      <ItemGrid ref={grid} items={items} />
      {showRail && <LetterRail titles={titles} grid={grid} />}
    </div>
  );
}

export function SearchPage() {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const { data, loading } = useApi<ItemCard[]>(q ? `api/items?q=${encodeURIComponent(q)}` : null, [q]);
  return (
    <div className="space-y-6">
      <Input
        autoFocus
        defaultValue={q}
        placeholder={t("search.placeholder")}
        onChange={(e) => setParams(e.target.value ? { q: e.target.value } : {}, { replace: true })}
        className="h-12 max-w-xl text-base"
        aria-label={t("nav.search")}
      />
      {q && loading && !data && <Spinner />}
      {q && data && data.length === 0 && <Empty title={t("search.nothing_found", { query: q })} />}
      {q && data && <ItemGrid items={data} />}
    </div>
  );
}
