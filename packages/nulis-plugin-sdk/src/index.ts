export const PLUGIN_API_VERSION = 1 as const;

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export interface Disposable {
  dispose(): void;
}

export type CommandHandler = (...arguments_: JsonValue[]) => JsonValue | Promise<JsonValue>;

export interface CommandsApi {
  registerCommand(id: string, handler: CommandHandler): Disposable;
}

export interface WindowApi {
  showInformationMessage(message: string): Promise<void>;
}

/** Capability-scoped API supplied by the Nulis host during activation. */
export interface NulisApi {
  readonly apiVersion: typeof PLUGIN_API_VERSION;
  readonly commands: CommandsApi;
  readonly window: WindowApi;
}

export interface ExtensionContext {
  readonly subscriptions: Disposable[];
}

export type Activate = (nulis: NulisApi, context: ExtensionContext) => void | Promise<void>;

export type PluginPermission = "showMessage";

export interface PluginCommandContribution {
  command: string;
  title: string;
  category?: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  engines: { nulis: string };
  permissions?: PluginPermission[];
  activationEvents?: string[];
  main: string;
  contributes?: { commands?: PluginCommandContribution[] };
}

/** Preserves literal manifest fields while checking the public API shape. */
export function definePlugin<const T extends PluginManifest>(manifest: T): T {
  return manifest;
}
