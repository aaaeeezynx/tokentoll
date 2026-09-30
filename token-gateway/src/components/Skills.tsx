/**
 * 技能（P3.3，對標 cc-switch 的 Skills Management）。
 *
 * 技能＝一個資料夾（內含 `SKILL.md`）。母本放在儲存目錄，再同步到各工具的
 * `skills/` 目錄（symlink 優先、失敗則複製 —— Windows 沒開開發者模式時
 * 建立目錄連結會回「用戶端沒有這項特殊權限」，退回複製是常態）。
 *
 * 來源是 GitHub 儲存庫（owner／name／branch／subdir）。更新偵測用**內容雜湊**比對。
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type InstalledSkill } from "../lib/api";
import { Icon } from "../components/icons";
import { useConfirm } from "../components/Confirm";
import { Toggle } from "../components/Toggle";
import { PopSelect } from "../components/PopSelect";
import { DiscoverPanel } from "./skills/DiscoverPanel";

const APPS = ["claude", "codex", "opencode"];
const APP_LABEL: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
};

export default function SkillsPage() {
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [showDiscover, setShowDiscover] = useState(true);
  /** 安裝時要同步到哪些工具（預設全選） */
  const [installApps, setInstallApps] = useState<string[]>(APPS);

  const list = useQuery({ queryKey: ["skills_list"], queryFn: api.skillsList });
  const backups = useQuery({ queryKey: ["skills_backups"], queryFn: api.skillsBackups });
  const settings = useQuery({ queryKey: ["skills_settings"], queryFn: api.skillsSettings });

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["skills_list"] });
    void qc.invalidateQueries({ queryKey: ["skills_backups"] });
  };

  const ok = (m: string) => {
    setMsg(m);
    setErr("");
    refresh();
  };
  const bad = (e: unknown) => setErr(String(e));

  const bind = useMutation({
    mutationFn: (v: { id: number; app: string; enabled: boolean }) =>
      api.skillsSetBinding(v.id, v.app, v.enabled),
    onSuccess: (r) =>
      ok(
        r.copied_fallback.length > 0
          ? `已同步（連結失敗改複製：${r.copied_fallback[0]}）`
          : `已同步 ${r.linked.length} 個目標`,
      ),
    onError: bad,
  });
  const update = useMutation({
    mutationFn: (name: string) => api.skillsUpdate(name),
    onSuccess: (o) => ok(o.unchanged ? `「${o.skill}」已是最新` : `「${o.skill}」已更新`),
    onError: bad,
  });
  const updateAll = useMutation({
    mutationFn: api.skillsUpdateAll,
    onSuccess: (rs) =>
      ok(
        rs.length === 0
          ? "沒有可更新的技能"
          : rs.map((r) => `${r.skill}：${r.message}`).join("；"),
      ),
    onError: bad,
  });
  const uninstall = useMutation({
    mutationFn: (name: string) => api.skillsUninstall(name),
    onSuccess: (r) => ok(`已解除安裝（備份：${r.backup}）`),
    onError: bad,
  });
  const restore = useMutation({
    mutationFn: (backup: string) => api.skillsRestore(backup, installApps),
    onSuccess: (o) => ok(`已從備份還原「${o.skill}」`),
    onError: bad,
  });
  const delBackup = useMutation({
    mutationFn: (backup: string) => api.skillsBackupDelete(backup),
    onSuccess: () => ok("已刪除備份"),
    onError: bad,
  });
  const setSettings = useMutation({
    mutationFn: (v: { storage?: string; syncMode?: string }) =>
      api.skillsSetSettings(v.storage, v.syncMode),
    onSuccess: () => {
      ok("已更新技能設定（既有的技能會在下次同步時套用）");
      void qc.invalidateQueries({ queryKey: ["skills_settings"] });
    },
    onError: bad,
  });

  const skills = list.data ?? [];
  const installedNames = useMemo(() => new Set(skills.map((s) => s.name)), [skills]);

  return (
    <div className="space-y-4">
      {dialog}
      <div className="glass p-5">
        <div className="mb-3 flex flex-wrap items-center gap-2.5">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/70">
            <Icon name="cpu" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[15px] font-semibold tracking-tight text-fg">
              技能
            </div>
            <div className="text-[11px] text-fg/30">
              從 GitHub 儲存庫安裝；母本存在儲存目錄，再同步到各工具的 skills 資料夾
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
              disabled={updateAll.isPending || skills.length === 0}
              onClick={() => updateAll.mutate()}
              title="檢查所有已安裝技能的遠端內容（用雜湊比對）"
            >
              <Icon name="refresh" size={12} />
              全部更新
            </button>
            <button
              className="btn-ghost px-3 py-1.5 text-xs"
              onClick={() => setShowDiscover((v) => !v)}
            >
              {showDiscover ? "收起探索" : "探索技能"}
            </button>
          </div>
        </div>

        {msg && <p className="pb-2 text-xs text-emerald-400/80">{msg}</p>}
        {err && <p className="pb-2 text-xs break-words text-red-400">{err}</p>}

        {/* 安裝時要同步到哪些工具 */}
        <div className="mb-3 flex flex-wrap items-center gap-3 rounded-lg bg-fg/[0.03] px-3 py-2">
          <span className="text-[11px] text-fg/40">安裝後同步到</span>
          {APPS.map((a) => (
            <label key={a} className="flex items-center gap-1.5 text-[12px] text-fg/65">
              <input
                type="checkbox"
                className="accent-[#0A84FF]"
                checked={installApps.includes(a)}
                onChange={(e) =>
                  setInstallApps((cur) =>
                    e.target.checked ? [...new Set([...cur, a])] : cur.filter((x) => x !== a),
                  )
                }
              />
              {APP_LABEL[a]}
            </label>
          ))}
          <span className="flex-1" />
          <span className="flex items-center gap-2 text-[11px] text-fg/35">
            儲存位置
            <span className="w-40">
              <PopSelect
                value={settings.data?.storage ?? "builtin"}
                onChange={(v) => setSettings.mutate({ storage: v })}
                options={[
                  { value: "builtin", label: "內建目錄（App 資料夾）" },
                  { value: "agents", label: "~/.agents/skills（共用）" },
                ]}
              />
            </span>
            同步方式
            <span className="w-32">
              <PopSelect
                value={settings.data?.sync_mode ?? "symlink"}
                onChange={(v) => setSettings.mutate({ syncMode: v })}
                options={[
                  { value: "symlink", label: "連結（預設）" },
                  { value: "copy", label: "複製" },
                ]}
              />
            </span>
          </span>
        </div>

        {list.isPending ? (
          <p className="text-sm text-fg/30">載入中…</p>
        ) : skills.length === 0 ? (
          <p className="text-[13px] leading-relaxed text-fg/35">
            還沒有安裝任何技能。展開「探索技能」從內建儲存庫（Anthropic 官方技能）挑一個裝。
          </p>
        ) : (
          <div className="space-y-2">
            {skills.map((s) => (
              <SkillRow
                key={s.id}
                s={s}
                busy={bind.isPending || update.isPending}
                onToggle={(app, enabled) => bind.mutate({ id: s.id, app, enabled })}
                onUpdate={() => update.mutate(s.name)}
                onUninstall={() =>
                  ask(`解除安裝技能「${s.name}」？`, () => uninstall.mutate(s.name), {
                    message:
                      "母本會先備份到 skill-backups，再從所有工具的 skills 目錄移除。可以從備份還原。",
                    confirmLabel: "解除安裝",
                  })
                }
              />
            ))}
          </div>
        )}

        <p className="pt-3 text-[11px] leading-relaxed text-fg/25">
          Claude Code 同步到 <span className="font-mono">~/.claude/skills/</span>；
          Codex <span className="font-mono">~/.codex/skills/</span>；
          OpenCode <span className="font-mono">~/.config/opencode/skills/</span>。
          更新偵測用內容雜湊（SHA-256）比對，不是看時間戳。
        </p>
      </div>

      {showDiscover && (
        <DiscoverPanel
          installApps={installApps}
          installedNames={installedNames}
          onDone={ok}
          onErr={bad}
        />
      )}

      {/* 備份（cc-switch 的 Restore from Backup） */}
      <div className="glass p-5">
        <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">
          技能備份
        </div>
        {(backups.data?.length ?? 0) === 0 ? (
          <p className="text-[13px] text-fg/35">
            還沒有備份 —— 解除安裝技能時會自動備份母本到這裡（保留全部，可手動刪）。
          </p>
        ) : (
          <div className="space-y-1.5">
            {backups.data?.map((b) => (
              <div
                key={b.name}
                className="flex flex-wrap items-center gap-2 rounded-md bg-fg/[0.04] px-3 py-2"
              >
                <span className="text-[13px] text-fg/80">{b.skill}</span>
                <span className="font-mono text-[10px] text-fg/30">{b.at}</span>
                <span className="text-[11px] text-fg/30">
                  {(b.bytes / 1024).toFixed(1)} KB
                </span>
                <span className="flex-1" />
                <button
                  className="btn-ghost px-2.5 py-1 text-[11px] disabled:opacity-40"
                  disabled={restore.isPending}
                  onClick={() => restore.mutate(b.name)}
                >
                  還原
                </button>
                <button
                  className="btn-ghost px-2.5 py-1 text-[11px] text-red-400/80"
                  disabled={delBackup.isPending}
                  onClick={() =>
                    ask("刪除這個備份？", () => delBackup.mutate(b.name), {
                      message: "刪除後無法復原。",
                    })
                  }
                >
                  刪除
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function SkillRow(props: {
  s: InstalledSkill;
  busy: boolean;
  onToggle: (app: string, enabled: boolean) => void;
  onUpdate: () => void;
  onUninstall: () => void;
}) {
  const { s } = props;
  const on = (app: string) => s.bindings.find((b) => b.app === app)?.enabled ?? false;
  return (
    <div className="rounded-xl border border-fg/[0.06] p-3.5 transition-colors hover:border-fg/10">
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1">
          <span className="truncate text-sm font-semibold text-fg/85">{s.name}</span>
          {s.repo_label && (
            <span className="ml-2 rounded bg-fg/[0.06] px-1.5 py-px text-[10px] text-fg/40">
              {s.repo_label}
            </span>
          )}
        </span>
        <span className="shrink-0 text-[10px] text-fg/25">
          {(s.size / 1024).toFixed(1)} KB
        </span>
        <button
          className="btn-ghost shrink-0 px-2.5 py-1 text-[11px] disabled:opacity-40"
          disabled={props.busy || s.repo_id == null}
          onClick={props.onUpdate}
          title={s.repo_id == null ? "沒有來源儲存庫，無法更新" : "檢查遠端內容並更新"}
        >
          更新
        </button>
        <button
          className="shrink-0 rounded-full p-1.5 text-fg/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="解除安裝（會先備份）"
          onClick={props.onUninstall}
        >
          <Icon name="trash" size={13} />
        </button>
      </div>
      {s.description && (
        <p className="mt-1 line-clamp-2 text-[11px] text-fg/35">{s.description}</p>
      )}
      <div className="mt-2 flex flex-wrap items-center gap-4">
        {APPS.map((a) => (
          <label
            key={a}
            className="flex items-center gap-1.5 text-[11px] text-fg/50"
            title={`同步到 ${APP_LABEL[a]}`}
          >
            <Toggle
              checked={on(a)}
              onChange={(v) => props.onToggle(a, v)}
              size="sm"
              disabled={props.busy}
            />
            {APP_LABEL[a]}
          </label>
        ))}
      </div>
    </div>
  );
}
