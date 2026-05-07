import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({ breaks: true, gfm: true });

/**
 * Render trusted-but-stored markdown into sanitized HTML.
 *
 * Runbooks are admin-authored but persisted in the DB and editable via the
 * API surface, so DOMPurify is mandatory defense-in-depth — an admin who
 * pastes `<script>` from an untrusted wiki should not produce stored XSS
 * in the panel's incident detail view.
 */
export function renderMarkdown(md: string): string {
  if (!md) return "";
  const raw = marked.parse(md, { async: false }) as string;
  return DOMPurify.sanitize(raw);
}
