import { plugin } from "bun";
import { compile, compileModule } from "svelte/compiler";
// Install browser globals before Bun evaluates any compiled Svelte component.
import "./dom";

plugin({
  name: "svelte",
  setup(build) {
    build.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
      const source = await Bun.file(path).text();
      return {
        contents: compile(source, { filename: path, generate: "client" }).js
          .code,
        loader: "js",
      };
    });
    build.onLoad({ filter: /\.svelte\.ts$/ }, async ({ path }) => {
      const source = await Bun.file(path).text();
      const javascript = new Bun.Transpiler({ loader: "ts" }).transformSync(
        source,
      );
      return {
        contents: compileModule(javascript, {
          filename: path,
          generate: "client",
        }).js.code,
        loader: "js",
      };
    });
  },
});
