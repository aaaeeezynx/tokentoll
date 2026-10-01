/** 擴充面板 API（用量查詢、MCP、提示詞、技能、會話）。 */
import { invoke } from "@tauri-apps/api/core";
import type {
  McpImportReport,
  McpInput,
  McpPreset,
  McpServer,
  McpSyncOutcome,
  PromptApp,
  PromptBackfill,
  PromptInput,
  PromptPanelState,
  PromptPreset,
  QuotaView,
  SkillBackup,
  SkillDiscoverResult,
  SkillInstallOutcome,
  SkillRepo,
  SkillSettings,
  SkillSyncReport,
  InstalledSkill,
  UsageQueryConfig,
  UsageTemplate,
} from "../apiTypes";

export const panelsApi = {
  // ── P2.1 用量查詢 ──
  usageQueryGet: (providerId: number): Promise<UsageQueryConfig> =>
    invoke("usage_query_get", { providerId }),
  usageQuerySet: (config: UsageQueryConfig): Promise<UsageQueryConfig> =>
    invoke("usage_query_set", { config }),
  usageQueryClear: (providerId: number): Promise<void> =>
    invoke("usage_query_clear", { providerId }),
  usageQueryTemplates: (): Promise<UsageTemplate[]> =>
    invoke("usage_query_templates"),
  usageQueryApplyTemplate: (
    providerId: number,
    template: string,
  ): Promise<UsageQueryConfig> =>
    invoke("usage_query_apply_template", { providerId, template }),
  /** 真的打一次查詢（面板的「測試」與卡片的「重新查詢」共用）。 */
  usageQueryRun: (providerId: number): Promise<QuotaView> =>
    invoke("usage_query_run", { providerId }),
  /** 所有已啟用設定的來源各查一次（來源頁載入時）。 */
  usageQueryRunAll: (): Promise<QuotaView[]> => invoke("usage_query_run_all"),
  // ── P3.1 MCP 管理 ──
  mcpList: (): Promise<McpServer[]> => invoke("mcp_list"),
  mcpPresets: (): Promise<McpPreset[]> => invoke("mcp_presets"),
  mcpUpsert: (input: McpInput): Promise<McpServer> =>
    invoke("mcp_upsert", { input }),
  mcpDelete: (id: number): Promise<McpSyncOutcome[]> =>
    invoke("mcp_delete", { id }),
  /** 設定某個伺服器在某個工具上的啟用（改完立刻同步設定檔）。 */
  mcpSetBinding: (
    id: number,
    target: string,
    enabled: boolean,
  ): Promise<McpSyncOutcome[]> =>
    invoke("mcp_set_binding", { id, target, enabled }),
  /** 一鍵把某個工具的所有伺服器開或關。 */
  mcpSetAppAll: (target: string, enabled: boolean): Promise<McpSyncOutcome[]> =>
    invoke("mcp_set_app_all", { target, enabled }),
  mcpSync: (): Promise<McpSyncOutcome[]> => invoke("mcp_sync"),
  mcpImport: (): Promise<McpImportReport> => invoke("mcp_import"),
  // ── P3.2 提示詞預設集 ──
  promptApps: (): Promise<PromptApp[]> => invoke("prompt_apps"),
  /** 面板狀態（順便做首次啟動匯入）。 */
  promptState: (app: string): Promise<PromptPanelState> =>
    invoke("prompt_state", { app }),
  promptList: (app: string): Promise<PromptPreset[]> =>
    invoke("prompt_list", { app }),
  promptSave: (input: PromptInput): Promise<PromptPreset> =>
    invoke("prompt_save", { input }),
  /** 啟用（切換前會先把檔案內容回填到舊的預設集）。 */
  promptActivate: (id: number): Promise<PromptBackfill> =>
    invoke("prompt_activate", { id }),
  promptDeactivate: (target: string): Promise<void> =>
    invoke("prompt_deactivate", { target }),
  promptDelete: (id: number): Promise<void> => invoke("prompt_delete", { id }),
  promptSync: (target: string): Promise<PromptBackfill> =>
    invoke("prompt_sync", { target }),
  /** 讀目前檔案內容（唯讀）。 */
  promptLive: (target: string): Promise<string> =>
    invoke("prompt_live", { target }),
  // ── P3.3 技能管理 ──
  skillsRepos: (): Promise<SkillRepo[]> => invoke("skills_repos"),
  skillsRepoAdd: (
    owner: string,
    name: string,
    branch?: string,
    subdir?: string,
    label?: string,
  ): Promise<SkillRepo> =>
    invoke("skills_repo_add", {
      owner,
      name,
      branch: branch ?? null,
      subdir: subdir ?? null,
      label: label ?? null,
    }),
  skillsRepoDelete: (id: number): Promise<void> =>
    invoke("skills_repo_delete", { id }),
  /** 掃描所有儲存庫（會打 GitHub）。 */
  skillsDiscover: (): Promise<SkillDiscoverResult> =>
    invoke("skills_discover"),
  skillsInstall: (args: {
    repo_id: number;
    remote_path: string;
    name: string;
    apps: string[];
  }): Promise<SkillInstallOutcome> => invoke("skills_install", { args }),
  skillsList: (): Promise<InstalledSkill[]> => invoke("skills_list"),
  skillsSetBinding: (
    id: number,
    target: string,
    enabled: boolean,
  ): Promise<SkillSyncReport> =>
    invoke("skills_set_binding", { id, target, enabled }),
  skillsUpdate: (name: string): Promise<SkillInstallOutcome> =>
    invoke("skills_update", { name }),
  skillsUpdateAll: (): Promise<{ skill: string; ok: boolean; message: string }[]> =>
    invoke("skills_update_all"),
  skillsUninstall: (
    name: string,
  ): Promise<{ removed: string[]; backup: string }> =>
    invoke("skills_uninstall", { name }),
  skillsBackups: (): Promise<SkillBackup[]> => invoke("skills_backups"),
  skillsRestore: (backup: string, apps: string[]): Promise<SkillInstallOutcome> =>
    invoke("skills_restore", { backup, apps }),
  skillsBackupDelete: (backup: string): Promise<void> =>
    invoke("skills_backup_delete", { backup }),
  skillsSettings: (): Promise<SkillSettings> => invoke("skills_settings"),
  skillsSetSettings: (
    storage?: string,
    syncMode?: string,
  ): Promise<SkillSettings> =>
    invoke("skills_set_settings", {
      storage: storage ?? null,
      syncMode: syncMode ?? null,
    }),
};
