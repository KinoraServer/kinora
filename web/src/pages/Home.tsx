import { Link } from "react-router";
import { MediaRow } from "@/components/ItemTile";
import { Empty, ErrorNote, Spinner } from "@/components/ui/card";
import { useApi, useSession } from "@/lib/session";
import type { HomeRow } from "@/lib/types";

export function HomePage() {
  const { data, error, loading } = useApi<HomeRow[]>("api/home");
  const { isAdmin } = useSession();

  if (loading && !data) return <Spinner className="mx-auto mt-24" />;
  if (error) return <ErrorNote message={error} />;
  if (!data?.length) {
    return (
      <Empty title="Nothing here yet">
        {isAdmin ? (
          <>
            Add a library in{" "}
            <Link to="/admin/libraries" className="cursor-pointer text-primary underline">
              Admin → Libraries
            </Link>{" "}
            to start scanning your media.
          </>
        ) : (
          "Ask the server owner to add a library."
        )}
      </Empty>
    );
  }
  return (
    <div className="space-y-10">
      {data.map((row) => (
        <MediaRow key={row.id} title={row.title} items={row.items} wide={row.id === "continue"} />
      ))}
    </div>
  );
}
