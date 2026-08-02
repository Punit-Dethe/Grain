export interface Language {
  value: string;
  label: string;
}

// The bare recognition code for Chinese — used to collapse zh-Hans/zh-Hant to
// the single code the engine sees. Kept as a constant so LanguageSelector.tsx
// can filter it out of the picker (all three recognize identically; showing the
// plain entry would only add ambiguity about which script you get).
export const CHINESE_LANGUAGE_CODE = "zh";

export const LANGUAGES: Language[] = [
  { value: "auto", label: "Auto Detect" },
  { value: "en", label: "English" },
  { value: "zh-Hans", label: "Simplified Chinese" },
  { value: "zh-Hant", label: "Traditional Chinese" },
  { value: "yue", label: "Cantonese" },
  { value: "de", label: "German" },
  { value: "es", label: "Spanish" },
  { value: "ru", label: "Russian" },
  { value: "ko", label: "Korean" },
  { value: "fr", label: "French" },
  { value: "ja", label: "Japanese" },
  { value: "pt", label: "Portuguese" },
  { value: "tr", label: "Turkish" },
  { value: "pl", label: "Polish" },
  { value: "ca", label: "Catalan" },
  { value: "nl", label: "Dutch" },
  { value: "ar", label: "Arabic" },
  { value: "sv", label: "Swedish" },
  { value: "it", label: "Italian" },
  { value: "id", label: "Indonesian" },
  { value: "hi", label: "Hindi" },
  { value: "fi", label: "Finnish" },
  { value: "vi", label: "Vietnamese" },
  { value: "he", label: "Hebrew" },
  { value: "uk", label: "Ukrainian" },
  { value: "el", label: "Greek" },
  { value: "ms", label: "Malay" },
  { value: "cs", label: "Czech" },
  { value: "ro", label: "Romanian" },
  { value: "da", label: "Danish" },
  { value: "hu", label: "Hungarian" },
  { value: "ta", label: "Tamil" },
  { value: "no", label: "Norwegian" },
  { value: "th", label: "Thai" },
  { value: "ur", label: "Urdu" },
  { value: "hr", label: "Croatian" },
  { value: "bg", label: "Bulgarian" },
  { value: "lt", label: "Lithuanian" },
  { value: "la", label: "Latin" },
  { value: "mi", label: "Maori" },
  { value: "ml", label: "Malayalam" },
  { value: "cy", label: "Welsh" },
  { value: "sk", label: "Slovak" },
  { value: "te", label: "Telugu" },
  { value: "fa", label: "Persian" },
  { value: "lv", label: "Latvian" },
  { value: "bn", label: "Bengali" },
  { value: "sr", label: "Serbian" },
  { value: "az", label: "Azerbaijani" },
  { value: "sl", label: "Slovenian" },
  { value: "kn", label: "Kannada" },
  { value: "et", label: "Estonian" },
  { value: "mk", label: "Macedonian" },
  { value: "br", label: "Breton" },
  { value: "eu", label: "Basque" },
  { value: "is", label: "Icelandic" },
  { value: "hy", label: "Armenian" },
  { value: "ne", label: "Nepali" },
  { value: "mn", label: "Mongolian" },
  { value: "bs", label: "Bosnian" },
  { value: "kk", label: "Kazakh" },
  { value: "sq", label: "Albanian" },
  { value: "sw", label: "Swahili" },
  { value: "gl", label: "Galician" },
  { value: "mr", label: "Marathi" },
  { value: "pa", label: "Punjabi" },
  { value: "si", label: "Sinhala" },
  { value: "km", label: "Khmer" },
  { value: "sn", label: "Shona" },
  { value: "yo", label: "Yoruba" },
  { value: "so", label: "Somali" },
  { value: "af", label: "Afrikaans" },
  { value: "oc", label: "Occitan" },
  { value: "ka", label: "Georgian" },
  { value: "be", label: "Belarusian" },
  { value: "tg", label: "Tajik" },
  { value: "sd", label: "Sindhi" },
  { value: "gu", label: "Gujarati" },
  { value: "am", label: "Amharic" },
  { value: "yi", label: "Yiddish" },
  { value: "lo", label: "Lao" },
  { value: "uz", label: "Uzbek" },
  { value: "fo", label: "Faroese" },
  { value: "ht", label: "Haitian Creole" },
  { value: "ps", label: "Pashto" },
  { value: "tk", label: "Turkmen" },
  { value: "nn", label: "Nynorsk" },
  { value: "mt", label: "Maltese" },
  { value: "sa", label: "Sanskrit" },
  { value: "lb", label: "Luxembourgish" },
  { value: "my", label: "Myanmar" },
  { value: "bo", label: "Tibetan" },
  { value: "tl", label: "Tagalog" },
  { value: "mg", label: "Malagasy" },
  { value: "as", label: "Assamese" },
  { value: "tt", label: "Tatar" },
  { value: "haw", label: "Hawaiian" },
  { value: "ln", label: "Lingala" },
  { value: "ha", label: "Hausa" },
  { value: "ba", label: "Bashkir" },
  { value: "jw", label: "Javanese" },
  { value: "su", label: "Sundanese" },
];

// Languages offered in the transcription-language picker. Filters out the bare
// CHINESE_LANGUAGE_CODE ("zh") — Grain's list doesn't have a bare "zh" entry
// anyway, but keeping this export in sync with upstream so LanguageSelector.tsx
// always uses a named, stable constant rather than raw LANGUAGES.
export const SELECTABLE_LANGUAGES: Language[] = LANGUAGES.filter(
  (language) => language.value !== CHINESE_LANGUAGE_CODE,
);

// Collapse a BCP-47 language tag to its base subtag, dropping any region or
// script suffix: "en-US" → "en", "zh-CN" → "zh", "zh-Hant" → "zh".
// Bare codes ("haw") and three-letter codes pass through unchanged. Lets the
// picker match a model's real codes (full locales like "en-US" for models like
// Nemotron Streaming) against Grain's canonical bare-code LANGUAGES list.
export const recognitionLanguage = (languageCode: string): string => {
  const separatorIndex = languageCode.indexOf("-");
  return separatorIndex === -1
    ? languageCode
    : languageCode.slice(0, separatorIndex);
};

// Base-aware check: returns true if `supported` contains at least one entry
// whose base code matches the base code of `languageCode`. A model advertising
// "en-US" will correctly match a bare intent of "en", and vice versa.
export const supportsLanguageCode = (
  supported: string[],
  languageCode: string,
): boolean =>
  supported.some(
    (lang) => recognitionLanguage(lang) === recognitionLanguage(languageCode),
  );
