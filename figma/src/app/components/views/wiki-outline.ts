interface MarkdownNode {
  type: string;
  depth?: number;
  value?: string;
  alt?: string;
  url?: string;
  ordered?: boolean;
  spread?: boolean;
  children?: MarkdownNode[];
  data?: { hName?: string; hProperties?: Record<string, unknown> };
}

function headingText(node: MarkdownNode): string {
  if (node.type === "footnoteReference" || node.type === "html") return "";
  return node.value ?? node.alt ?? node.children?.map(headingText).join("") ?? "";
}

/** Parse actual Markdown headings so code examples and quoted headings never enter the outline. */
export function remarkWikiOutline({ idPrefix, label }: { idPrefix: string; label: string }) {
  return (tree: MarkdownNode) => {
    const headings = (tree.children ?? []).filter(node =>
      node.type === "heading" && (node.depth === 2 || node.depth === 3) && headingText(node).trim(),
    );
    const items = headings.map((heading, index) => {
      const id = `${idPrefix}section-${index}`;
      heading.data = { ...heading.data, hProperties: { ...heading.data?.hProperties, id } };
      return {
        type: "listItem", spread: false,
        children: [{ type: "paragraph", children: [{
          type: "link", url: `#${id}`,
          children: [{ type: "text", value: headingText(heading).trim() }],
        }] }],
      };
    });
    if (items.length < 2) return;
    tree.children!.unshift({
      type: "blockquote",
      data: { hName: "nav", hProperties: { "aria-label": label } },
      children: [
        { type: "paragraph", children: [{ type: "strong", children: [{ type: "text", value: label }] }] },
        { type: "list", ordered: false, spread: false, children: items },
      ],
    });
  };
}
