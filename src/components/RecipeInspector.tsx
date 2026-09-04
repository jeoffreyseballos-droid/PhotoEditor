import { useState } from "react";
import { api, errorMessage } from "../api";
import type { Asset } from "../types";
import type { MaskDiagnostic } from "../toolkit";
import type {
  EditRecipe,
  RecipeState,
  RecipeRevision,
  RecipeDifference,
} from "../recipe";

export function RecipeInspector({
  asset,
  state,
  recipe,
  mask,
  busy,
  onAction,
  unresolvedMasks,
}: {
  asset: Asset;
  state: RecipeState;
  recipe: EditRecipe;
  mask?: MaskDiagnostic;
  busy: boolean;
  unresolvedMasks?: string[] | null;
  onAction: (
    action: "snapshot" | "export" | "import" | "restore",
    revisionId?: string,
  ) => void;
}) {
  const [history, setHistory] = useState<RecipeRevision[]>([]);
  const [diff, setDiff] = useState<RecipeDifference[]>([]);
  const [json, setJson] = useState("");
  const [message, setMessage] = useState("");
  const [loading, setLoading] = useState(false);
  const pending = JSON.stringify(recipe) !== JSON.stringify(state.recipe);
  const localUnresolved = recipe.local_layers.filter(
    (l) =>
      l.enabled &&
      l.strength > 0 &&
      (l.mask_type === "custom" ||
        mask?.status !== "ready" ||
        (l.mask_reference !== null &&
          l.mask_reference.content_id !== mask.reference)),
  );
  const unresolved =
    !pending && unresolvedMasks
      ? unresolvedMasks
      : localUnresolved.map((l) => l.id);
  async function inspect(
    action: "history" | "json" | "diff",
    revisionId?: string,
  ) {
    setLoading(true);
    setMessage("");
    try {
      if (action === "history") {
        const rows = await api.recipeHistory(asset.job_id, asset.id);
        setHistory(rows);
        if (!rows.length) setMessage("No saved revisions.");
      } else if (action === "json") {
        const value = await api.recipeJson(asset.job_id, asset.id);
        setJson(value);
      } else if (revisionId) {
        const value = await api.recipeDiff(asset.job_id, asset.id, revisionId);
        setDiff(value);
        if (!value.length) setMessage("No control changes from this revision.");
      }
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }
  return (
    <details className="recipe-inspector">
      <summary>Recipe Inspector</summary>
      <dl>
        <dt>Schema</dt>
        <dd>{recipe.schema_version}</dd>
        <dt>Recipe ID</dt>
        <dd>{recipe.recipe_id}</dd>
        <dt>Recipe hash</dt>
        <dd>{state.recipe_hash}</dd>
        <dt>Origin</dt>
        <dd>{recipe.provenance.origin}</dd>
        <dt>Current revision</dt>
        <dd>{state.current_revision}</dd>
        <dt>Local layers</dt>
        <dd>{recipe.local_layers.length}</dd>
        <dt>Unresolved masks</dt>
        <dd>{unresolved.length ? unresolved.join(", ") : "None"}</dd>
        <dt>Modified</dt>
        <dd>
          {pending
            ? "Saving / not yet validated"
            : state.modified
              ? "Draft differs from snapshot"
              : "Saved snapshot"}
        </dd>
      </dl>
      {state.error && <p className="error">{state.error.message}</p>}
      <p className="muted">
        One recipe for this photo. Masks are disposable, asset-specific data. No
        trained style or AI decisions.
      </p>
      <fieldset disabled={busy || loading || pending}>
        <legend>Recipe tools</legend>
        <button disabled={!!state.error} onClick={() => onAction("snapshot")}>
          Save Snapshot
        </button>
        <button disabled={!!state.error} onClick={() => void inspect("json")}>
          View Recipe JSON
        </button>
        <button disabled={!!state.error} onClick={() => onAction("export")}>
          Export Recipe JSON
        </button>
        <button onClick={() => onAction("import")}>Import Recipe JSON</button>
        <button onClick={() => void inspect("history")}>
          Load Revision History
        </button>
        {history.length > 0 && (
          <ol>
            {history.map((r) => (
              <li key={r.revision_id}>
                Revision {r.revision_number} — {r.reason.replaceAll("_", " ")} ·{" "}
                {r.created_at}
                <button onClick={() => void inspect("diff", r.revision_id)}>
                  Compare revision {r.revision_number}
                </button>
                <button onClick={() => onAction("restore", r.revision_id)}>
                  Restore revision {r.revision_number}
                </button>
              </li>
            ))}
          </ol>
        )}
      </fieldset>
      <p className="muted">
        History loads on request (latest 100 shown). Initial plus latest 199
        meaningful snapshots retained; slider ticks and auto-preview do not add
        revisions.
      </p>
      {message && <p role="status">{message}</p>}
      {diff.length > 0 && (
        <table>
          <caption>Selected revision → current recipe</caption>
          <thead>
            <tr>
              <th>Control</th>
              <th>Before</th>
              <th>After</th>
            </tr>
          </thead>
          <tbody>
            {diff.map((d, i) => (
              <tr key={i}>
                <td>{d.control}</td>
                <td>{JSON.stringify(d.before)}</td>
                <td>{JSON.stringify(d.after)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {json && (
        <label>
          Canonical Recipe JSON
          <textarea readOnly rows={12} value={json} />
        </label>
      )}
    </details>
  );
}
