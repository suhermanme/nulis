import { useEffect, useState } from "react";
import type { DocumentSummary, RemoteDirectory, RemoteEntry, RemoteProfile } from "./contracts";
import { executeCommand } from "./ipc";

interface RemotePanelProps {
  onOpen: (document: DocumentSummary) => void;
  onStatus: (message: string) => void;
}

const emptyProfile: RemoteProfile = {
  id: "", label: "", host: "", port: 22, username: "", rootPath: "/",
  credentialId: "", hostKeySha256: "",
};

function message(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function RemotePanel({ onOpen, onStatus }: RemotePanelProps) {
  const [profiles, setProfiles] = useState<RemoteProfile[]>([]);
  const [profile, setProfile] = useState<RemoteProfile>(emptyProfile);
  const [password, setPassword] = useState("");
  const [directory, setDirectory] = useState<RemoteDirectory | null>(null);

  const loadProfiles = async () => {
    try {
      const next = await executeCommand<RemoteProfile[]>({
        id: "remote.listProfiles", context: { windowId: "main" }, arguments: {},
      });
      setProfiles(next);
      if (!profile.id && next[0]) setProfile(next[0]);
    } catch (error: unknown) {
      onStatus(message(error, "Could not load remote profiles"));
    }
  };
  useEffect(() => { void loadProfiles(); }, []); // Profiles are refreshed explicitly after mutations.

  const save = async () => {
    const next = { ...profile, credentialId: profile.credentialId || `${profile.id}.password` };
    try {
      if (password) {
        await executeCommand({
          id: "remote.setCredential", context: { windowId: "main" },
          arguments: { credentialId: next.credentialId, secret: password },
        });
      }
      const saved = await executeCommand<RemoteProfile>({
        id: "remote.saveProfile", context: { windowId: "main" }, arguments: next,
      });
      setProfile(saved);
      setPassword("");
      await loadProfiles();
      onStatus(`Saved remote profile ${saved.label}`);
    } catch (error: unknown) {
      onStatus(message(error, "Could not save remote profile"));
    }
  };

  const list = async (path: string) => {
    if (!profile.id) return;
    onStatus(`Connecting to ${profile.label || profile.host}…`);
    try {
      const result = await executeCommand<RemoteDirectory>({
        id: "remote.list", context: { windowId: "main" },
        arguments: { profileId: profile.id, path },
      });
      setDirectory(result);
      onStatus(`Remote ${result.path || "/"}`);
    } catch (error: unknown) {
      onStatus(message(error, "Could not list remote directory"));
    }
  };

  const openEntry = async (entry: RemoteEntry) => {
    if (entry.kind === "directory") { await list(entry.path); return; }
    if (entry.kind !== "file") return;
    try {
      const opened = await executeCommand<DocumentSummary>({
        id: "remote.openFile", context: { windowId: "main" },
        arguments: { profileId: profile.id, path: entry.path },
      });
      onOpen(opened);
      onStatus(`Opened ${opened.remoteUri ?? opened.name}`);
    } catch (error: unknown) {
      onStatus(message(error, "Could not open remote file"));
    }
  };

  const parentPath = directory?.path.split("/").slice(0, -1).join("/") ?? "";
  return <div className="remote-panel">
    <div className="panel-heading"><span>REMOTE · SFTP</span><button type="button" onClick={() => {
      setProfile(emptyProfile); setDirectory(null); setPassword("");
    }}>New</button></div>
    {profiles.length > 0 ? <select aria-label="Remote profile" value={profile.id}
      onChange={(event) => {
        const selected = profiles.find((candidate) => candidate.id === event.target.value);
        if (selected) { setProfile(selected); setDirectory(null); }
      }}>
      <option value="">New profile</option>
      {profiles.map((candidate) => <option value={candidate.id} key={candidate.id}>{candidate.label}</option>)}
    </select> : null}
    <div className="remote-form">
      <input aria-label="Profile ID" placeholder="profile-id" value={profile.id}
        onChange={(event) => setProfile({ ...profile, id: event.target.value })} />
      <input aria-label="Profile label" placeholder="Label" value={profile.label}
        onChange={(event) => setProfile({ ...profile, label: event.target.value })} />
      <input aria-label="Remote host" placeholder="Host" value={profile.host}
        onChange={(event) => setProfile({ ...profile, host: event.target.value })} />
      <div><input aria-label="Remote username" placeholder="Username" value={profile.username}
        onChange={(event) => setProfile({ ...profile, username: event.target.value })} />
      <input aria-label="Remote port" type="number" min="1" max="65535" value={profile.port}
        onChange={(event) => setProfile({ ...profile, port: Number(event.target.value) })} /></div>
      <input aria-label="Remote root path" placeholder="Root path" value={profile.rootPath}
        onChange={(event) => setProfile({ ...profile, rootPath: event.target.value })} />
      <input aria-label="SHA-256 host key fingerprint" placeholder="Pinned host key SHA-256" value={profile.hostKeySha256}
        onChange={(event) => setProfile({ ...profile, hostKeySha256: event.target.value.toLowerCase().replaceAll(":", "") })} />
      <input aria-label="Remote password" type="password" autoComplete="new-password"
        placeholder={profile.credentialId ? "New password (optional)" : "Password"} value={password}
        onChange={(event) => setPassword(event.target.value)} />
      <div className="remote-actions"><button type="button" onClick={() => void save()}>Save</button>
        <button type="button" disabled={!profile.id} onClick={() => void list("")}>Connect</button></div>
    </div>
    {directory ? <div className="remote-browser">
      <button type="button" disabled={!directory.path} onClick={() => void list(parentPath)}>← {directory.path || "/"}</button>
      {directory.entries.map((entry) => <button type="button" key={entry.path}
        disabled={entry.kind === "other" || entry.kind === "symlink"} onClick={() => void openEntry(entry)}>
        <span>{entry.kind === "directory" ? "▸" : "·"} {entry.name}</span>
        <small>{entry.byteLen ?? entry.kind}</small>
      </button>)}
      {directory.truncated ? <p>First 1,000 entries shown</p> : null}
    </div> : null}
  </div>;
}
