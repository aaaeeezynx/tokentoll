/**
 * 技能探索（P3.3）：儲存庫管理 ＋ 掃描遠端技能清單。
 *
 * 掃描會真的打 GitHub（contents API），列出每個儲存庫 `subdir` 底下的技能資料夾；
 * 安裝會下載整個儲存庫的 tarball 再挑出那個資料夾（一次請求，比逐檔抓省得多）。
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type RemoteSkill } from "../../lib/api";
import { Icon } from "../icons";
import { PopSelect } from "../PopSelect";

export function DiscoverPanel(props: {
  installApps: string[];
  installedNames: Set<string>;
  onDone: (msg: string) => void;
  onErr: (e: unknown) => void;
}) {
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState("all");
  const [showRepos, setShowRepos] = useState(false);
  const [repoForm, setRepoForm] = useState({ owner: "", name: "", branch: "main", subdir: "", label: "" });

  const repos = useQuery({ queryKey: ["skills_repos"], queryFn: api.skillsRepos });
  const discovered = useQuery({
    queryKey: ["skills_discover"],
    queryFn: api.skillsDiscover,
    // 掃描會打 GitHub：只在按「重新整理」時重抓，切回來不重打
    staleTime: 5 * 60_000,
    refetchOnWindowFocus: false,
  });

  const scan = useMutation({
    mutationFn: api.skillsDiscover,
    onSuccess: (r) => {
      qc.setQueryData(["skills_discover"], r);
      props.onDone(
        `掃描完成：找到 ${r.skills.length} 個技能` +
          (r.errors.length > 0 ? `；${r.errors.length} 個儲存庫讀不到` : ""),
      );
    },
    onError: props.onErr,
  });
  const install = useMutation({
    mutationFn: (s: RemoteSkill) =>
      api.skillsInstall({
        repo_id: s.repo_id,
        remote_path: s.path,
        name: s.name,
        apps: props.installApps,
      }),
    onSuccess: (o) => {
      props.onDone(
        `已安裝「${o.skill}」（${o.files} 個檔案` +
          (o.sync.linked.length > 0 ? `，同步到 ${o.sync.linked.length} 個工具` : "") +
          (o.sync.copied_fallback.length > 0 ? "，連結失敗改複製" : "") +
          "）",
      );
      void qc.invalidateQueries({ queryKey: ["skills_discover"] });
    },
    onError: props.onErr,
  });
  const addRepo = useMutation({
    mutationFn: () =>
      api.skillsRepoAdd(
        repoForm.owner,
        repoForm.name,
        repoForm.branch,
        repoForm.subdir,
        repoForm.label,
      ),
    onSuccess: (r) => {
      props.onDone(`已新增儲存庫 ${r.owner}/${r.name}`);
      setRepoForm({ owner: "", name: "", branch: "main", subdir: "", label: "" });
      void qc.invalidateQueries({ queryKey: ["skills_repos"] });
    },
    onError: props.onErr,
  });
  const delRepo = useMutation({
    mutationFn: (id: number) => api.skillsRepoDelete(id),
    onSuccess: () => {
      props.onDone("已刪除儲存庫");
      void qc.invalidateQueries({ queryKey: ["skills_repos"] });
    },
    onError: props.onErr,
  });

  const list = useMemo(() => {
    const all = discovered.data?.skills ?? [];
    const s = q.trim().toLowerCase();
    return all.filter((x) => {
      if (filter === "installed" && !x.installed) return false;
      if (filter === "missing" && x.installed) return false;
      if (!s) return true;
      return (
        x.name.toLowerCase().includes(s) ||
        x.repo_label.toLowerCase().includes(s) ||
        x.path.toLowerCase().includes(s)
      );
    });
  }, [discovered.data, q, filter]);

  return (
    <div className="glass p-5">
      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold tracking-tight text-fg/85">探索技能</div>
          <div className="text-[11px] text-fg/30">
            從 GitHub 儲存庫列出可用技能（掃描會真的連網；tarball 下載安裝）
          </div>
        </div>
        <button className="btn-ghost px-3 py-1.5 text-xs" onClick={() => setShowRepos((v) => !v)}>
          儲存庫（{repos.data?.length ?? 0}）
        </button>
        <button
          className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={scan.isPending}
          onClick={() => scan.mutate()}
        >
          <Icon name="refresh" size={12} />
          {scan.isPending ? "掃描中…" : "重新整理"}
        </button>
      </div>

      {showRepos && (
        <div className="mb-3 space-y-2 rounded-lg bg-fg/[0.03] p-3">
          {(repos.data ?? []).map((r) => (
            <div key={r.id} className="flex flex-wrap items-center gap-2 text-[12px]">
              <span className="text-fg/75">{r.label || `${r.owner}/${r.name}`}</span>
              <span className="font-mono text-[10px] text-fg/30">
                {r.owner}/{r.name}@{r.branch}
                {r.subdir ? `/${r.subdir}` : ""}
              </span>
              {r.builtin ? (
                <span className="rounded bg-fg/[0.06] px-1.5 py-px text-[10px] text-fg/35">
                  內建
                </span>
              ) : (
                <button
                  className="btn-ghost px-2 py-0.5 text-[11px] text-red-400/80"
                  onClick={() => delRepo.mutate(r.id)}
                >
                  刪除
                </button>
              )}
            </div>
          ))}
          <div className="flex flex-wrap items-center gap-1.5 pt-1">
            {(
              [
                ["owner", "擁有者（例如 anthropics）"],
                ["name", "儲存庫名"],
                ["branch", "分支（預設 main）"],
                ["subdir", "子目錄（技能放在哪）"],
                ["label", "顯示名稱（選填）"],
              ] as const
            ).map(([k, ph]) => (
              <input
                key={k}
                className="field px-2 py-1 text-[12px]"
                placeholder={ph}
                value={repoForm[k]}
                onChange={(e) => setRepoForm((f) => ({ ...f, [k]: e.target.value }))}
              />
            ))}
            <button
              className="btn-ghost px-3 py-1 text-[11px] disabled:opacity-40"
              disabled={addRepo.isPending || !repoForm.owner || !repoForm.name}
              onClick={() => addRepo.mutate()}
            >
              新增儲存庫
            </button>
          </div>
        </div>
      )}

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <input
          className="field min-w-[200px] flex-1 px-2.5 py-1.5 text-[12px]"
          placeholder="搜尋技能名稱或路徑…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <div className="w-36">
          <PopSelect
            value={filter}
            onChange={setFilter}
            options={[
              { value: "all", label: "全部" },
              { value: "installed", label: "已安裝" },
              { value: "missing", label: "未安裝" },
            ]}
          />
        </div>
      </div>

      {discovered.isPending ? (
        <p className="text-sm text-fg/30">掃描中…（第一次會打 GitHub）</p>
      ) : discovered.isError ? (
        <p className="text-sm break-words text-fg/50">
          掃描失敗：{String(discovered.error)}
        </p>
      ) : list.length === 0 ? (
        <p className="text-[13px] text-fg/35">
          {(discovered.data?.skills.length ?? 0) === 0
            ? "還沒有掃描結果 —— 按右上角「重新整理」打一次 GitHub。"
            : "沒有符合條件的技能。"}
        </p>
      ) : (
        <>
          <p className="pb-2 text-[11px] text-fg/30">符合 {list.length} 個</p>
          <div className="grid gap-1.5 sm:grid-cols-2">
            {list.slice(0, 200).map((s) => (
              <div
                key={`${s.repo_id}:${s.path}`}
                className="flex items-center gap-2 rounded-lg border border-fg/[0.06] px-3 py-2"
              >
                <span className="min-w-0 flex-1">
                  <span className="truncate text-[13px] text-fg/80">{s.name}</span>
                  <span className="block truncate font-mono text-[10px] text-fg/25">
                    {s.repo_label} · {s.path}
                  </span>
                </span>
                {s.installed ? (
                  <span
                    className="shrink-0 rounded px-1.5 py-px text-[10px]"
                    style={{ background: "rgba(48,209,88,0.14)", color: "#30d158" }}
                  >
                    已安裝
                  </span>
                ) : (
                  <button
                    className="btn-ghost shrink-0 px-2.5 py-1 text-[11px] disabled:opacity-40"
                    disabled={install.isPending}
                    onClick={() => install.mutate(s)}
                  >
                    安裝
                  </button>
                )}
              </div>
            ))}
          </div>
        </>
      )}

      {(discovered.data?.errors.length ?? 0) > 0 && (
        <div className="pt-3 text-[11px] leading-relaxed text-amber-300/80">
          {discovered.data?.errors.map(([label, why]) => (
            <div key={label}>
              {label}：{why}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
