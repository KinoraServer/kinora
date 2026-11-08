import { Trans, useTranslation } from "react-i18next";
import { Link } from "react-router";
import { MediaRow } from "@/components/ItemTile";
import { Empty, ErrorNote, Spinner } from "@/components/ui/card";
import { useApi, useSession } from "@/lib/session";
import type { HomeRow } from "@/lib/types";

export function HomePage() {
  const { t } = useTranslation();
  const { data, error, loading } = useApi<HomeRow[]>("api/home");
  const { isAdmin } = useSession();

  if (loading && !data) return <Spinner className="mx-auto mt-24" />;
  if (error) return <ErrorNote message={error} />;
  if (!data?.length) {
    return (
      <Empty title={t("home.empty_title")}>
        {isAdmin ? (
          <Trans
            i18nKey="home.empty_admin"
            components={{ link: <Link to="/admin/libraries" className="cursor-pointer text-primary underline" /> }}
          />
        ) : (
          t("home.empty_viewer")
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
