import i18n from "i18next"
import { initReactI18next } from "react-i18next"
import LanguageDetector from "i18next-browser-languagedetector"
import en from "./locales/en/translation.json"
import ar from "./locales/ar/translation.json"

const resources = {
  en: { translation: en },
  ar: { translation: ar },
}

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: "en",
    supportedLngs: ["en", "ar"],
    debug: import.meta.env.DEV,
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
    },
  })

// Set document direction based on language
i18n.on("languageChanged", (lng) => {
  const dir = lng === "ar" ? "rtl" : "ltr"
  document.documentElement.dir = dir
  document.documentElement.lang = lng
})

// Set initial direction
const dir = i18n.language === "ar" ? "rtl" : "ltr"
document.documentElement.dir = dir
document.documentElement.lang = i18n.language

export default i18n
