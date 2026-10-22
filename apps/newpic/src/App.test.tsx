import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { App } from "./App";

describe("App", () => {
  it("renders the NewPic settings heading", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toBe("<main>NewPic 설정</main>");
  });
});
