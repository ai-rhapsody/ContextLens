import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Rich } from "./Rich";

const html = (t: string) => renderToStaticMarkup(<Rich text={t} />);

describe("Rich", () => {
  it("renders the Markdown models use instead of showing symbols", () => {
    expect(html("**粗體** 和 *斜體*")).toBe("<p><strong>粗體</strong> 和 <em>斜體</em></p>");
    expect(html("### 小標")).toBe("<h3>小標</h3>");
    expect(html("1. 一\n2. 二")).toBe("<ol><li>一</li><li>二</li></ol>");
    expect(html("- a\n* b")).toBe("<ul><li>a</li><li>b</li></ul>");
  });

  it("leaves arithmetic and snake_case alone", () => {
    expect(html("2 * 3 * 4")).toBe("<p>2 * 3 * 4</p>");
    expect(html("use snake_case_names")).toBe("<p>use snake_case_names</p>");
  });

  it("never injects HTML", () => {
    expect(html("<img src=x onerror=alert(1)>")).toBe("<p>&lt;img src=x onerror=alert(1)&gt;</p>");
  });
});
