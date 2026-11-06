import { useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { ItemGrid } from "@/components/ItemTile";
import { Empty, ErrorNote, Spinner } from "@/components/ui/card";
import { Input, Select } from "@/components/ui/input";
import { useApi } from "@/lib/session";
import type { ItemCard, Library } from "@/lib/types";

export function LibraryPage() {
  const { id } = useParams();
  const [sort, setSort] = useState("title");
  const [filter, setFilter] = useState("");
  const { data: libraries } = useApi<Library[]>("api/libraries");
  const library = libraries?.find((l) => l.id === id);
  const { data, error, loading } = useApi<ItemCard[]>(`api/items?library_id=${id}&sort=${sort}`, [id, sort]);
  const items = data?.filter((i) => i.title.toLowerCase().includes(filter.toLowerCase())) ?? [];

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">{library?.name ?? "Library"}</h1>
          {data && <p className="text-sm text-muted-foreground">{data.length} titles</p>}
        </div>
        <div className="flex gap-2">
          <Input
            placeholder="Filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="w-44"
            aria-label="Filter"
          />
          <Select value={sort} onChange={(e) => setSort(e.target.value)} className="w-40" aria-label="Sort">
            <option value="title">Title</option>
            <option value="added">Recently added</option>
            <option value="year">Year</option>
            <option value="rating">Rating</option>
          </Select>
        </div>
      </div>
      {loading && !data && <Spinner className="mx-auto mt-16" />}
      <ErrorNote message={error} />
      {data && items.length === 0 && (
        <Empty title={data.length ? "No matches" : "This library is empty"}>
          {data.length ? "Try a different filter." : "If you just added it, the scan may still be running."}
        </Empty>
      )}
      <ItemGrid items={items} />
    </div>
  );
}

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const { data, loading } = useApi<ItemCard[]>(q ? `api/items?q=${encodeURIComponent(q)}` : null, [q]);
  return (
    <div className="space-y-6">
      <Input
        autoFocus
        defaultValue={q}
        placeholder="Search movies and shows"
        onChange={(e) => setParams(e.target.value ? { q: e.target.value } : {}, { replace: true })}
        className="h-12 max-w-xl text-base"
        aria-label="Search"
      />
      {q && loading && !data && <Spinner />}
      {q && data && data.length === 0 && <Empty title={`Nothing found for "${q}"`} />}
      {q && data && <ItemGrid items={data} />}
    </div>
  );
}
