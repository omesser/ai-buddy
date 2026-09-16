// The linter rung the issue never considered: no types, just the two rules that
// cover both bugs this webview has actually shipped (#738).
//
// Not a proposal to adopt eslint — section 3 of REPORT.md argues the opposite,
// because this config costs 194 packages where `typescript` costs 1. It exists
// so the comparison in section 2 is reproducible:
//
//   npm install eslint@9 globals eslint-plugin-import
//   npx eslint --no-config-lookup -c spike/typescript-692/eslint.config.probe.mjs src
//
// On current `src/`: 0 errors, 1 real dead-variable warning at main.js:176.
import globals from "globals";
import importPlugin from "eslint-plugin-import";

export default [
  {
    files: ["**/*.js"],
    // Byte-for-byte npm output; a fixer here would break its pinned hash.
    ignores: ["**/vendor/**"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: { ...globals.browser },
    },
    plugins: { import: importPlugin },
    rules: {
      "no-undef": "error",
      "import/named": "error",
      "no-unused-vars": "warn",
    },
    settings: { "import/resolver": { node: { extensions: [".js"] } } },
  },
];
