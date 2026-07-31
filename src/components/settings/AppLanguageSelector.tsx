import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import {
  SUPPORTED_LANGUAGES,
  isSupportedLanguage,
  setLanguage,
  type SupportedLanguageCode,
} from "../../i18n";
import { useSettings } from "@/hooks/useSettings";

interface AppLanguageSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AppLanguageSelector: React.FC<AppLanguageSelectorProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t, i18n } = useTranslation();
    // [GRAIN] Catalogues load on demand, so switching goes through setLanguage
    // (fetch, then switch). Calling i18n.changeLanguage directly would switch to
    // a language i18next has no resources for and paint English.
    const { settings, updateSetting } = useSettings();

    // The stored preference is already a resolved code — the user picked it
    // from this very list — so this is a membership test, not a resolution.
    // Resolving a raw tag is the backend's job (i18n/index.ts: resolveLocale).
    const currentLanguage = (isSupportedLanguage(settings?.app_language)
      ? settings!.app_language!
      : i18n.language) as SupportedLanguageCode;

    const languageOptions = SUPPORTED_LANGUAGES.map((lang) => ({
      value: lang.code,
      label: `${lang.nativeName} (${lang.name})`,
    }));

    const handleLanguageChange = (langCode: string) => {
      void setLanguage(langCode);
      updateSetting("app_language", langCode);
    };

    return (
      <SettingContainer
        title={t("appLanguage.title")}
        description={t("appLanguage.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={languageOptions}
          selectedValue={currentLanguage}
          onSelect={handleLanguageChange}
        />
      </SettingContainer>
    );
  });

AppLanguageSelector.displayName = "AppLanguageSelector";
