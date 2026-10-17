import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";
import { changeLocale } from "../i18n";
beforeEach(() => {
  changeLocale("de");
  localStorage.clear();
});
afterEach(cleanup);
