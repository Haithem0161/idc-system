import i18n from "i18next"
import { initReactI18next } from "react-i18next"
import LanguageDetector from "i18next-browser-languagedetector"

import enCommon from "./locales/en/common.json"
import enErrors from "./locales/en/errors.json"
import enReceipts from "./locales/en/receipts.json"
import enLegacy from "./locales/en/translation.json"
import arCommon from "./locales/ar/common.json"
import arErrors from "./locales/ar/errors.json"
import arReceipts from "./locales/ar/receipts.json"
import arLegacy from "./locales/ar/translation.json"

// Phase-01 §7.10: split locales into namespaces. The legacy `translation.json`
// stays as the default namespace for backwards compatibility with existing
// pages until each phase migrates its strings.
const resources = {
  en: {
    translation: { ...enLegacy, ...enCommon, ...enErrors, ...enReceipts },
    common: enCommon,
    errors: enErrors,
    receipts: enReceipts,
  },
  ar: {
    translation: { ...arLegacy, ...arCommon, ...arErrors, ...arReceipts },
    common: arCommon,
    errors: arErrors,
    receipts: arReceipts,
  },
}

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: "en",
    supportedLngs: ["en", "ar"],
    ns: ["translation", "common", "errors", "receipts"],
    defaultNS: "translation",
    debug: import.meta.env.DEV,
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
    },
  })

function applyDirection (language: string) {
  const dir = language === "ar" ? "rtl" : "ltr"
  document.documentElement.dir = dir
  document.documentElement.lang = language
}

i18n.on("languageChanged", applyDirection)
applyDirection(i18n.language)

export default i18n
