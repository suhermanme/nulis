import { keepFocusInDialog } from "./dialogFocus";
import { useRestoreFocus } from "./Picker";

/** Asks for a file's new name; focus returns to where it was when it closes. */
export function RenameDialog({ name, busy, onSubmit, onCancel }: {
  name: string; busy: boolean; onSubmit(name: string): void; onCancel(): void;
}) {
  useRestoreFocus();
  return <div className="decision-backdrop">
    <form className="decision-dialog" role="dialog" aria-modal="true" aria-labelledby="rename-file-title"
      onSubmit={(event) => {
        event.preventDefault();
        const field = event.currentTarget.elements.namedItem("name");
        if (field instanceof HTMLInputElement) onSubmit(field.value);
      }}
      onKeyDown={(event) => {
        keepFocusInDialog(event);
        if (event.key === "Escape") onCancel();
      }}>
      <h2 id="rename-file-title">Rename {name}</h2>
      <p>The file is renamed in its folder. Unsaved changes stay unsaved.</p>
      <input name="name" aria-label="New file name" defaultValue={name} autoFocus spellCheck={false}
        onFocus={(event) => {
          // Select the name without its extension, as file managers do.
          const dot = event.currentTarget.value.lastIndexOf(".");
          event.currentTarget.setSelectionRange(0, dot > 0 ? dot : event.currentTarget.value.length);
        }} />
      <div className="decision-actions">
        <button type="button" disabled={busy} onClick={onCancel}>Cancel</button>
        <button className="primary" type="submit" disabled={busy}>Rename</button>
      </div>
    </form>
  </div>;
}
