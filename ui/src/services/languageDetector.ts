/**
 * VoxForge Language Detector
 * Real-time script & dialect detection with support for:
 * - Native Indic scripts (Malayalam, Hindi, Tamil, Telugu, Kannada, Bengali, Marathi, Gujarati, Urdu, Punjabi)
 * - Romanized Indic Dialects (Manglish, Hinglish, Tanglish, Kanglish, Tenglish)
 * - World languages (English, Spanish, French, German, Japanese, Chinese)
 */

import languages646Raw from '../constants/languages646.json';

export interface DetectedLanguage {
  code: string;
  name: string;
  flag: string;
  isRomanizedDialect: boolean;
  nativeTargetCode?: string;
  suggestedVoiceId?: string;
  confidence: number;
  sampleKeywordsFound?: string[];
}

export interface LanguageOption {
  code: string;
  name: string;
  flag: string;
  nativeScript?: string;
  isRomanized?: boolean;
}

export const SUPPORTED_LANGUAGES: LanguageOption[] = [
  { code: 'AUTO', name: 'Auto-Detect Language', flag: '🌐' },

  // ── Indian Scripts & Romanized Dialects ───────────────────────────────
  { code: 'ml-Latn', name: 'Manglish (Malayalam Romanized)', flag: '🌴', isRomanized: true },
  { code: 'ml-IN',   name: 'Malayalam (മലയാളം)',              flag: '🇮🇳', nativeScript: 'മലയാളം' },
  { code: 'hi-Latn', name: 'Hinglish (Hindi Romanized)',      flag: '🇮🇳', isRomanized: true },
  { code: 'hi-IN',   name: 'Hindi (हिन्दी)',                  flag: '🇮🇳', nativeScript: 'हिन्दी' },
  { code: 'ta-Latn', name: 'Tanglish (Tamil Romanized)',      flag: '🇮🇳', isRomanized: true },
  { code: 'ta-IN',   name: 'Tamil (தமிழ்)',                   flag: '🇮🇳', nativeScript: 'தமிழ்' },
  { code: 'te-IN',   name: 'Telugu (తెలుగు)',                 flag: '🇮🇳', nativeScript: 'తెలుగు' },
  { code: 'kn-IN',   name: 'Kannada (ಕನ್ನಡ)',                 flag: '🇮🇳', nativeScript: 'ಕನ್ನಡ' },
  { code: 'bn-IN',   name: 'Bengali India (বাংলা)',           flag: '🇮🇳', nativeScript: 'বাংলা' },
  { code: 'bn-BD',   name: 'Bengali Bangladesh (বাংলা)',      flag: '🇧🇩', nativeScript: 'বাংলা' },
  { code: 'mr-IN',   name: 'Marathi (मराठी)',                 flag: '🇮🇳', nativeScript: 'मराठी' },
  { code: 'gu-IN',   name: 'Gujarati (ગુજરાતી)',              flag: '🇮🇳', nativeScript: 'ગુજરાતી' },
  { code: 'pa-IN',   name: 'Punjabi India (ਪੰਜਾਬੀ)',          flag: '🇮🇳', nativeScript: 'ਪੰਜਾਬੀ' },
  { code: 'ur-IN',   name: 'Urdu India (اردو)',               flag: '🇮🇳', nativeScript: 'اردو' },
  { code: 'ur-PK',   name: 'Urdu Pakistan (اردو)',            flag: '🇵🇰', nativeScript: 'اردو' },
  { code: 'or-IN',   name: 'Odia (ଓଡ଼ିଆ)',                   flag: '🇮🇳', nativeScript: 'ଓଡ଼ିଆ' },
  { code: 'ne-NP',   name: 'Nepali (नेपाली)',                 flag: '🇳🇵', nativeScript: 'नेपाली' },
  { code: 'si-LK',   name: 'Sinhala (සිංහල)',                 flag: '🇱🇰', nativeScript: 'සිංහල' },
  { code: 'my-MM',   name: 'Burmese (မြန်မာ)',                flag: '🇲🇲', nativeScript: 'မြန်မာ' },
  { code: 'km-KH',   name: 'Khmer (ខ្មែរ)',                   flag: '🇰🇭', nativeScript: 'ខ្មែរ' },
  { code: 'lo-LA',   name: 'Lao (ລາວ)',                       flag: '🇱🇦', nativeScript: 'ລາວ' },

  // ── English variants ───────────────────────────────────────────────────
  { code: 'en-US', name: 'English (US)',           flag: '🇺🇸' },
  { code: 'en-GB', name: 'English (UK)',           flag: '🇬🇧' },
  { code: 'en-AU', name: 'English (Australia)',    flag: '🇦🇺' },
  { code: 'en-CA', name: 'English (Canada)',       flag: '🇨🇦' },
  { code: 'en-NZ', name: 'English (New Zealand)',  flag: '🇳🇿' },
  { code: 'en-IN', name: 'English (India)',        flag: '🇮🇳' },
  { code: 'en-IE', name: 'English (Ireland)',      flag: '🇮🇪' },
  { code: 'en-ZA', name: 'English (South Africa)', flag: '🇿🇦' },
  { code: 'en-SG', name: 'English (Singapore)',   flag: '🇸🇬' },
  { code: 'en-NG', name: 'English (Nigeria)',      flag: '🇳🇬' },
  { code: 'en-KE', name: 'English (Kenya)',        flag: '🇰🇪' },
  { code: 'en-TZ', name: 'English (Tanzania)',     flag: '🇹🇿' },
  { code: 'en-PH', name: 'English (Philippines)',  flag: '🇵🇭' },
  { code: 'en-HK', name: 'English (Hong Kong)',    flag: '🇭🇰' },

  // ── East Asian ─────────────────────────────────────────────────────────
  { code: 'ja-JP',   name: 'Japanese (日本語)',          flag: '🇯🇵', nativeScript: '日本語' },
  { code: 'zh-CN',   name: 'Mandarin (Simplified 中文)',  flag: '🇨🇳', nativeScript: '中文' },
  { code: 'zh-TW',   name: 'Chinese Traditional (繁體)',  flag: '🇹🇼', nativeScript: '繁體中文' },
  { code: 'zh-HK',   name: 'Cantonese HK (粵語)',         flag: '🇭🇰', nativeScript: '粵語' },
  { code: 'ko-KR',   name: 'Korean (한국어)',              flag: '🇰🇷', nativeScript: '한국어' },
  { code: 'mn-MN',   name: 'Mongolian (Монгол)',           flag: '🇲🇳', nativeScript: 'Монгол' },

  // ── Southeast Asian ────────────────────────────────────────────────────
  { code: 'id-ID',   name: 'Indonesian (Bahasa)',   flag: '🇮🇩' },
  { code: 'ms-MY',   name: 'Malay (Bahasa Melayu)', flag: '🇲🇾' },
  { code: 'th-TH',   name: 'Thai (ภาษาไทย)',         flag: '🇹🇭', nativeScript: 'ภาษาไทย' },
  { code: 'vi-VN',   name: 'Vietnamese (Tiếng Việt)', flag: '🇻🇳' },
  { code: 'fil-PH',  name: 'Filipino (Tagalog)',     flag: '🇵🇭' },

  // ── Middle East & North Africa ─────────────────────────────────────────
  { code: 'ar-SA',   name: 'Arabic (Saudi العربية)',   flag: '🇸🇦', nativeScript: 'العربية' },
  { code: 'ar-EG',   name: 'Arabic (Egypt)',           flag: '🇪🇬', nativeScript: 'العربية' },
  { code: 'ar-AE',   name: 'Arabic (UAE)',             flag: '🇦🇪', nativeScript: 'العربية' },
  { code: 'ar-MA',   name: 'Arabic (Morocco)',         flag: '🇲🇦', nativeScript: 'العربية' },
  { code: 'fa-IR',   name: 'Persian / Farsi (فارسی)',  flag: '🇮🇷', nativeScript: 'فارسی' },
  { code: 'he-IL',   name: 'Hebrew (עברית)',           flag: '🇮🇱', nativeScript: 'עברית' },
  { code: 'tr-TR',   name: 'Turkish (Türkçe)',         flag: '🇹🇷' },

  // ── Western Europe ─────────────────────────────────────────────────────
  { code: 'de-DE',   name: 'German (Deutsch)',         flag: '🇩🇪' },
  { code: 'de-AT',   name: 'German (Austria)',         flag: '🇦🇹' },
  { code: 'de-CH',   name: 'German (Switzerland)',     flag: '🇨🇭' },
  { code: 'fr-FR',   name: 'French (Français)',        flag: '🇫🇷' },
  { code: 'fr-BE',   name: 'French (Belgium)',         flag: '🇧🇪' },
  { code: 'fr-CA',   name: 'French (Canada)',          flag: '🇨🇦' },
  { code: 'es-ES',   name: 'Spanish (España)',         flag: '🇪🇸' },
  { code: 'es-MX',   name: 'Spanish (Mexico)',         flag: '🇲🇽' },
  { code: 'es-AR',   name: 'Spanish (Argentina)',      flag: '🇦🇷' },
  { code: 'es-CO',   name: 'Spanish (Colombia)',       flag: '🇨🇴' },
  { code: 'es-US',   name: 'Spanish (US)',             flag: '🇺🇸' },
  { code: 'pt-BR',   name: 'Portuguese (Brasil)',      flag: '🇧🇷' },
  { code: 'pt-PT',   name: 'Portuguese (Portugal)',    flag: '🇵🇹' },
  { code: 'it-IT',   name: 'Italian (Italiano)',       flag: '🇮🇹' },
  { code: 'nl-NL',   name: 'Dutch (Nederlands)',       flag: '🇳🇱' },
  { code: 'nl-BE',   name: 'Dutch (Belgium)',          flag: '🇧🇪' },
  { code: 'sv-SE',   name: 'Swedish (Svenska)',        flag: '🇸🇪' },
  { code: 'nb-NO',   name: 'Norwegian (Norsk)',        flag: '🇳🇴' },
  { code: 'da-DK',   name: 'Danish (Dansk)',           flag: '🇩🇰' },
  { code: 'fi-FI',   name: 'Finnish (Suomi)',          flag: '🇫🇮' },
  { code: 'pl-PL',   name: 'Polish (Polski)',          flag: '🇵🇱' },
  { code: 'cs-CZ',   name: 'Czech (Čeština)',          flag: '🇨🇿' },
  { code: 'sk-SK',   name: 'Slovak (Slovenčina)',      flag: '🇸🇰' },
  { code: 'hu-HU',   name: 'Hungarian (Magyar)',       flag: '🇭🇺' },
  { code: 'ro-RO',   name: 'Romanian (Română)',        flag: '🇷🇴' },
  { code: 'bg-BG',   name: 'Bulgarian (Български)',    flag: '🇧🇬', nativeScript: 'Български' },
  { code: 'hr-HR',   name: 'Croatian (Hrvatski)',      flag: '🇭🇷' },
  { code: 'sl-SI',   name: 'Slovenian (Slovenščina)',  flag: '🇸🇮' },
  { code: 'sr-RS',   name: 'Serbian (Српски)',         flag: '🇷🇸', nativeScript: 'Српски' },
  { code: 'bs-BA',   name: 'Bosnian (Bosanski)',       flag: '🇧🇦' },
  { code: 'mk-MK',   name: 'Macedonian (Македонски)',  flag: '🇲🇰', nativeScript: 'Македонски' },
  { code: 'sq-AL',   name: 'Albanian (Shqip)',         flag: '🇦🇱' },
  { code: 'mt-MT',   name: 'Maltese (Malti)',          flag: '🇲🇹' },
  { code: 'is-IS',   name: 'Icelandic (Íslenska)',     flag: '🇮🇸' },
  { code: 'lv-LV',   name: 'Latvian (Latviešu)',       flag: '🇱🇻' },
  { code: 'lt-LT',   name: 'Lithuanian (Lietuvių)',    flag: '🇱🇹' },
  { code: 'et-EE',   name: 'Estonian (Eesti)',         flag: '🇪🇪' },
  { code: 'ga-IE',   name: 'Irish Gaelic (Gaeilge)',   flag: '🇮🇪' },
  { code: 'cy-GB',   name: 'Welsh (Cymraeg)',          flag: '🏴󠁧󠁢󠁷󠁬󠁳󠁿' },
  { code: 'ca-ES',   name: 'Catalan (Català)',         flag: '🏳️' },
  { code: 'gl-ES',   name: 'Galician (Galego)',        flag: '🏳️' },
  { code: 'eu-ES',   name: 'Basque (Euskara)',         flag: '🏳️' },

  // ── Eastern Europe / Central Asia ─────────────────────────────────────
  { code: 'ru-RU',   name: 'Russian (Русский)',        flag: '🇷🇺', nativeScript: 'Русский' },
  { code: 'uk-UA',   name: 'Ukrainian (Українська)',   flag: '🇺🇦', nativeScript: 'Українська' },
  { code: 'be-BY',   name: 'Belarusian (Беларуская)',  flag: '🇧🇾', nativeScript: 'Беларуская' },
  { code: 'kk-KZ',   name: 'Kazakh (Қазақша)',         flag: '🇰🇿', nativeScript: 'Қазақша' },
  { code: 'uz-UZ',   name: 'Uzbek (Oʻzbekcha)',        flag: '🇺🇿' },
  { code: 'ky-KG',   name: 'Kyrgyz (Кыргызча)',        flag: '🇰🇬', nativeScript: 'Кыргызча' },
  { code: 'tk-TM',   name: 'Turkmen (Türkmen)',        flag: '🇹🇲' },
  { code: 'az-AZ',   name: 'Azerbaijani (Azərbaycan)', flag: '🇦🇿' },
  { code: 'ka-GE',   name: 'Georgian (ქართული)',        flag: '🇬🇪', nativeScript: 'ქართული' },
  { code: 'hy-AM',   name: 'Armenian (Հայերեն)',        flag: '🇦🇲', nativeScript: 'Հայերեն' },

  // ── Africa ─────────────────────────────────────────────────────────────
  { code: 'af-ZA',   name: 'Afrikaans',                flag: '🇿🇦' },
  { code: 'sw-KE',   name: 'Swahili (Kenya)',          flag: '🇰🇪' },
  { code: 'sw-TZ',   name: 'Swahili (Tanzania)',       flag: '🇹🇿' },
  { code: 'am-ET',   name: 'Amharic (አማርኛ)',           flag: '🇪🇹', nativeScript: 'አማርኛ' },
  { code: 'so-SO',   name: 'Somali (Soomaali)',        flag: '🇸🇴' },
  { code: 'zu-ZA',   name: 'Zulu (isiZulu)',           flag: '🇿🇦' },
  { code: 'yo-NG',   name: 'Yoruba',                   flag: '🇳🇬' },
  { code: 'ha-NG',   name: 'Hausa',                    flag: '🇳🇬' },
  { code: 'ig-NG',   name: 'Igbo',                     flag: '🇳🇬' },
  { code: 'ps-AF',   name: 'Pashto (پښتو)',            flag: '🇦🇫', nativeScript: 'پښتو' },
];

export const ALL_646_LANGUAGES: LanguageOption[] = [
  ...SUPPORTED_LANGUAGES,
  ...languages646Raw
    .filter(
      (l) =>
        !SUPPORTED_LANGUAGES.some(
          (s) =>
            s.code.toLowerCase().startsWith(l.id.toLowerCase()) ||
            s.code.toLowerCase() === l.iso.toLowerCase() ||
            s.name.toLowerCase().includes(l.name.toLowerCase())
        )
    )
    .map((l) => ({
      code: l.id,
      name: `${l.name} (${l.iso.toUpperCase()})`,
      flag: '🌐',
    })),
];

export const REGIONAL_DIALECTS: Record<string, Array<{ code: string; label: string }>> = {
  en: [
    { code: 'en-US', label: 'US' },
    { code: 'en-GB', label: 'UK' },
    { code: 'en-IN', label: 'India' },
    { code: 'en-AU', label: 'Australia' },
  ],
  es: [
    { code: 'es-ES', label: 'Spain' },
    { code: 'es-MX', label: 'Mexico' },
    { code: 'es-AR', label: 'Argentina' },
    { code: 'es-CO', label: 'Colombia' },
  ],
  pt: [
    { code: 'pt-BR', label: 'Brazil' },
    { code: 'pt-PT', label: 'Portugal' },
  ],
  fr: [
    { code: 'fr-FR', label: 'France' },
    { code: 'fr-CA', label: 'Canada' },
  ],
  de: [
    { code: 'de-DE', label: 'Germany' },
    { code: 'de-AT', label: 'Austria' },
    { code: 'de-CH', label: 'Switzerland' },
  ],
  ar: [
    { code: 'ar-SA', label: 'Saudi Arabia' },
    { code: 'ar-EG', label: 'Egypt' },
    { code: 'ar-MA', label: 'Morocco' },
  ],
  zh: [
    { code: 'zh-CN', label: 'Simplified' },
    { code: 'zh-TW', label: 'Traditional' },
    { code: 'zh-HK', label: 'Cantonese' },
  ],
};

const MACRO_ISO_MAP: Record<string, string> = {
  arb: 'ar', arz: 'ar', acm: 'ar', apc: 'ar', ary: 'ar', ajp: 'ar',
  cmn: 'zh', zho: 'zh', yue: 'zh',
  mal: 'ml', hin: 'hi', tam: 'ta', tel: 'te', kan: 'kn', ben: 'bn',
  urd: 'ur', npi: 'ne', jpn: 'ja', kor: 'ko', fra: 'fr', deu: 'de',
  spa: 'es', por: 'pt', rus: 'ru', ita: 'it', nld: 'nl', tur: 'tr',
  vie: 'vi', tha: 'th', ell: 'el', heb: 'he', amh: 'am', kat: 'ka', hye: 'hy',
};

export function normalizeLanguageCode(code?: string): string {
  if (!code || code === 'AUTO') return 'AUTO';
  const parts = code.trim().replace('_', '-').split('-').filter(Boolean);
  if (parts.length === 0) return 'AUTO';
  const base = MACRO_ISO_MAP[parts[0].toLowerCase()] || parts[0].toLowerCase();
  if (parts.length >= 2) {
    if (parts[1].toLowerCase() === 'latn') return base;
    return `${base}-${parts[1].toUpperCase()}`;
  }
  return base;
}

// Lexicon for Romanized Manglish (Malayalam written in Latin script)
const MANGLISH_KEYWORDS = new Set([
  'adipoli', 'adpoli', 'kidilam', 'machane', 'chunke', 'pwoli', 'sceneanu', 'sceneaanu',
  'enthokke', 'vishesham', 'njan', 'njangal', 'ninte', 'ningal', 'ivide', 'avide', 'evide', 'evideya',
  'poda', 'mone', 'chetta', 'chechi', 'aano', 'undu', 'illa', 'raksha', 'poli', 'valare',
  'nannayi', 'manassilayi', 'engane', 'ippo', 'ippol', 'kollam', 'thanne', 'sathyam', 'pinne',
  'nokku', 'parayu', 'parayam', 'enthaanu', 'entha', 'kidu', 'chumma', 'parayamo',
  'ariyilla', 'sheriyaanu', 'sheriyaa', 'veruthe', 'thante', 'cheyyanam', 'enthoru', 'nalla',
  'kollaam', 'thannathaane', 'kaaryam', 'sukhamano', 'namaskaram', 'oru', 'ithu', 'athu', 'nammude',
  'naale', 'innu', 'innale', 'varumo', 'poyalo', 'kelkkamo', 'setaayi', 'set aayi', 'aliya', 'aliyan',
  'dhaivame', 'ente', 'ponno', 'enteponno', 'kollalo', 'polichu'
]);

// Lexicon for Romanized Hinglish (Hindi written in Latin script)
const HINGLISH_KEYWORDS = new Set([
  'bhai', 'kya', 'haal', 'chal', 'ekdum', 'mast', 'achha', 'accha', 'theek', 'shukriya',
  'dhanyawad', 'yaar', 'kaise', 'kahan', 'karo', 'suno', 'dekho', 'kuch', 'nahi', 'nahin',
  'hota', 'samajh', 'gaya', 'chalega', 'bahut', 'badhiya', 'jhakaas', 'arre', 'arey', 'bolo',
  'apna', 'apne', 'kaam', 'karna', 'kardo', 'zara', 'sunao', 'sabko', 'hoga', 'namaste',
  'dost', 'chalo', 'batao', 'sahi', 'hai', 'hain', 'mujhe', 'tumhe', 'humlog', 'aap', 'mera',
  'humara', 'karega', 'khabar', 'paani', 'arrewaah', 'waah'
]);

// Lexicon for Romanized Tanglish (Tamil written in Latin script)
const TANGLISH_KEYWORDS = new Set([
  'machi', 'vanakkam', 'epdi', 'irukka', 'semma', 'vera', 'veralevel', 'nandri',
  'theriyum', 'solla', 'vaanga', 'ponga', 'thalaiva', 'romba', 'illai', 'podu',
  'paravala', 'solunga', 'kudunga', 'nanba', 'nanbi', 'ennada', 'enna', 'inga', 'anga',
  'seri', 'amma', 'appa', 'machan', 'kandippa', 'apdiya', 'kelunga'
]);

// Lexicon for European common words
const SPANISH_KEYWORDS = new Set([
  'hola', 'buenos', 'dias', 'tardes', 'noches', 'gracias', 'por', 'favor', 'amigo', 'como',
  'estas', 'bien', 'donde', 'esta', 'adios', 'hasta', 'luego', 'senor', 'senora'
]);

const FRENCH_KEYWORDS = new Set([
  'bonjour', 'salut', 'merci', 'beaucoup', 'comment', 'allez', 'vous', 'bien', 'oui',
  'non', 's\'il', 'vous', 'plait', 'au', 'revoir', 'monsieur', 'madame'
]);

const GERMAN_KEYWORDS = new Set([
  'hallo', 'guten', 'tag', 'morgen', 'abend', 'danke', 'bitte', 'wie', 'geht\'s', 'tschuss',
  'auf', 'wiedersehen', 'ja', 'nein', 'freund', 'schon', 'wunderbar'
]);

export function detectLanguage(text: string): DetectedLanguage {
  const trimmed = text.trim();
  if (!trimmed) {
    return {
      code: 'en-US',
      name: 'English (US)',
      flag: '🇺🇸',
      isRomanizedDialect: false,
      suggestedVoiceId: 'en-US-AriaNeural',
      confidence: 1.0,
    };
  }

  // 1. Check Native Script Unicode Ranges
  let malayalamChars = 0;
  let devanagariChars = 0;
  let tamilChars = 0;
  let teluguChars = 0;
  let kannadaChars = 0;
  let bengaliChars = 0;
  let gujaratiChars = 0;
  let gurmukhiChars = 0;
  let arabicChars = 0;
  let cjkChars = 0;
  let japaneseChars = 0;
  let koreanChars = 0;
  let cyrillicChars = 0;
  let greekChars = 0;
  let hebrewChars = 0;
  let thaiChars = 0;
  let georgianChars = 0;
  let armenianChars = 0;
  let ethiopicChars = 0;
  let sinhalaChars = 0;
  let khmerChars = 0;
  let laoChars = 0;
  let myanmarChars = 0;
  let totalScriptChars = 0;

  for (let i = 0; i < trimmed.length; i++) {
    const code = trimmed.charCodeAt(i);
    // Malayalam: U+0D00 - U+0D7F
    if (code >= 0x0d00 && code <= 0x0d7f) {
      malayalamChars++;
      totalScriptChars++;
    }
    // Devanagari (Hindi, Marathi): U+0900 - U+097F
    else if (code >= 0x0900 && code <= 0x097f) {
      devanagariChars++;
      totalScriptChars++;
    }
    // Tamil: U+0B80 - U+0BFF
    else if (code >= 0x0b80 && code <= 0x0bff) {
      tamilChars++;
      totalScriptChars++;
    }
    // Telugu: U+0C00 - U+0C7F
    else if (code >= 0x0c00 && code <= 0x0c7f) {
      teluguChars++;
      totalScriptChars++;
    }
    // Kannada: U+0C80 - U+0CFF
    else if (code >= 0x0c80 && code <= 0x0cff) {
      kannadaChars++;
      totalScriptChars++;
    }
    // Bengali: U+0980 - U+09FF
    else if (code >= 0x0980 && code <= 0x09ff) {
      bengaliChars++;
      totalScriptChars++;
    }
    // Gujarati: U+0A80 - U+0AFF
    else if (code >= 0x0a80 && code <= 0x0aff) {
      gujaratiChars++;
      totalScriptChars++;
    }
    // Gurmukhi (Punjabi): U+0A00 - U+0A7F
    else if (code >= 0x0a00 && code <= 0x0a7f) {
      gurmukhiChars++;
      totalScriptChars++;
    }
    // Arabic / Urdu: U+0600 - U+06FF
    else if (code >= 0x0600 && code <= 0x06ff) {
      arabicChars++;
      totalScriptChars++;
    }
    // Korean Hangul syllables: U+AC00 - U+D7A3, Hangul Jamo: U+1100 - U+11FF
    else if ((code >= 0xac00 && code <= 0xd7a3) || (code >= 0x1100 && code <= 0x11ff)) {
      koreanChars++;
      totalScriptChars++;
    }
    // Japanese Hiragana & Katakana: U+3040 - U+30FF
    else if ((code >= 0x3040 && code <= 0x309f) || (code >= 0x30a0 && code <= 0x30ff)) {
      japaneseChars++;
      totalScriptChars++;
    }
    // CJK Unified Ideographs: U+4E00 - U+9FFF
    else if (code >= 0x4e00 && code <= 0x9fff) {
      cjkChars++;
      totalScriptChars++;
    }
    // Cyrillic: U+0400 - U+04FF
    else if (code >= 0x0400 && code <= 0x04ff) {
      cyrillicChars++;
      totalScriptChars++;
    }
    // Greek: U+0370 - U+03FF
    else if (code >= 0x0370 && code <= 0x03ff) {
      greekChars++;
      totalScriptChars++;
    }
    // Hebrew: U+0590 - U+05FF
    else if (code >= 0x0590 && code <= 0x05ff) {
      hebrewChars++;
      totalScriptChars++;
    }
    // Thai: U+0E00 - U+0E7F
    else if (code >= 0x0e00 && code <= 0x0e7f) {
      thaiChars++;
      totalScriptChars++;
    }
    // Georgian: U+10A0 - U+10FF
    else if (code >= 0x10a0 && code <= 0x10ff) {
      georgianChars++;
      totalScriptChars++;
    }
    // Armenian: U+0530 - U+058F
    else if (code >= 0x0530 && code <= 0x058f) {
      armenianChars++;
      totalScriptChars++;
    }
    // Ethiopic (Amharic): U+1200 - U+137F
    else if (code >= 0x1200 && code <= 0x137f) {
      ethiopicChars++;
      totalScriptChars++;
    }
    // Sinhala: U+0D80 - U+0DFF
    else if (code >= 0x0d80 && code <= 0x0dff) {
      sinhalaChars++;
      totalScriptChars++;
    }
    // Khmer: U+1780 - U+17FF
    else if (code >= 0x1780 && code <= 0x17ff) {
      khmerChars++;
      totalScriptChars++;
    }
    // Lao: U+0E80 - U+0EFF
    else if (code >= 0x0e80 && code <= 0x0eff) {
      laoChars++;
      totalScriptChars++;
    }
    // Myanmar: U+1000 - U+109F
    else if (code >= 0x1000 && code <= 0x109f) {
      myanmarChars++;
      totalScriptChars++;
    }
  }

  // Native Script Matches (dominant count)
  if (malayalamChars >= 2 || (malayalamChars > 0 && totalScriptChars === malayalamChars)) {
    return {
      code: 'ml-IN',
      name: 'Malayalam (മലയാളം)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'ml-IN-SobhanaNeural',
      confidence: Math.min(1.0, 0.7 + (malayalamChars / trimmed.length) * 0.3),
    };
  }

  if (tamilChars >= 2 || (tamilChars > 0 && totalScriptChars === tamilChars)) {
    return {
      code: 'ta-IN',
      name: 'Tamil (தமிழ்)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'ta-IN-PallaviNeural',
      confidence: Math.min(1.0, 0.7 + (tamilChars / trimmed.length) * 0.3),
    };
  }

  if (teluguChars >= 2 || (teluguChars > 0 && totalScriptChars === teluguChars)) {
    return {
      code: 'te-IN',
      name: 'Telugu (తెలుగు)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'te-IN-ShrutiNeural',
      confidence: Math.min(1.0, 0.7 + (teluguChars / trimmed.length) * 0.3),
    };
  }

  if (kannadaChars >= 2 || (kannadaChars > 0 && totalScriptChars === kannadaChars)) {
    return {
      code: 'kn-IN',
      name: 'Kannada (ಕನ್ನಡ)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'kn-IN-SapnaNeural',
      confidence: Math.min(1.0, 0.7 + (kannadaChars / trimmed.length) * 0.3),
    };
  }

  if (devanagariChars >= 2 || (devanagariChars > 0 && totalScriptChars === devanagariChars)) {
    return {
      code: 'hi-IN',
      name: 'Hindi (हिन्दी)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'hi-IN-SwaraNeural',
      confidence: Math.min(1.0, 0.7 + (devanagariChars / trimmed.length) * 0.3),
    };
  }

  if (bengaliChars >= 2 || (bengaliChars > 0 && totalScriptChars === bengaliChars)) {
    return {
      code: 'bn-IN',
      name: 'Bengali (বাংলা)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'bn-IN-TanishaaNeural',
      confidence: Math.min(1.0, 0.7 + (bengaliChars / trimmed.length) * 0.3),
    };
  }

  if (gujaratiChars >= 2 || (gujaratiChars > 0 && totalScriptChars === gujaratiChars)) {
    return {
      code: 'gu-IN',
      name: 'Gujarati (ગુજરાતી)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'gu-IN-DhwaniNeural',
      confidence: Math.min(1.0, 0.7 + (gujaratiChars / trimmed.length) * 0.3),
    };
  }

  if (gurmukhiChars >= 2 || (gurmukhiChars > 0 && totalScriptChars === gurmukhiChars)) {
    return {
      code: 'pa-IN',
      name: 'Punjabi (ਪੰਜਾਬੀ)',
      flag: '🇮🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'hi-IN-SwaraNeural',
      confidence: Math.min(1.0, 0.7 + (gurmukhiChars / trimmed.length) * 0.3),
    };
  }

  if (koreanChars >= 1) {
    return {
      code: 'ko-KR',
      name: 'Korean (한국어)',
      flag: '🇰🇷',
      isRomanizedDialect: false,
      suggestedVoiceId: 'ko-KR-SunHiNeural',
      confidence: Math.min(1.0, 0.7 + (koreanChars / trimmed.length) * 0.3),
    };
  }

  if (arabicChars >= 2 || (arabicChars > 0 && totalScriptChars === arabicChars)) {
    // Distinguish Arabic (ar-SA) from Urdu (ur-IN) based on context — default to Arabic
    // since we now surface ar-SA in the language selector
    return {
      code: 'ar-SA',
      name: 'Arabic (العربية)',
      flag: '🇸🇦',
      isRomanizedDialect: false,
      suggestedVoiceId: 'ar-SA-ZariyahNeural',
      confidence: Math.min(1.0, 0.7 + (arabicChars / trimmed.length) * 0.3),
    };
  }

  if (japaneseChars >= 1) {
    return {
      code: 'ja-JP',
      name: 'Japanese (日本語)',
      flag: '🇯🇵',
      isRomanizedDialect: false,
      suggestedVoiceId: 'ja-JP-NanamiNeural',
      confidence: 0.95,
    };
  }

  if (cjkChars >= 2) {
    return {
      code: 'zh-CN',
      name: 'Mandarin Chinese (中文)',
      flag: '🇨🇳',
      isRomanizedDialect: false,
      suggestedVoiceId: 'zh-CN-XiaoxiaoNeural',
      confidence: 0.95,
    };
  }

  const expandedScripts: Array<[number, string, string, string, string]> = [
    [cyrillicChars, 'ru-RU', 'Russian (Русский)', '🇷🇺', 'ru-RU-SvetlanaNeural'],
    [greekChars, 'el-GR', 'Greek (Ελληνικά)', '🇬🇷', 'el-GR-AthinaNeural'],
    [hebrewChars, 'he-IL', 'Hebrew (עברית)', '🇮🇱', 'he-IL-HilaNeural'],
    [thaiChars, 'th-TH', 'Thai (ไทย)', '🇹🇭', 'th-TH-PremwadeeNeural'],
    [georgianChars, 'ka-GE', 'Georgian (ქართული)', '🇬🇪', 'ka-GE-EkaNeural'],
    [armenianChars, 'hy-AM', 'Armenian (Հայերեն)', '🇦🇲', 'hy-AM-AnahitNeural'],
    [ethiopicChars, 'am-ET', 'Amharic (አማርኛ)', '🇪🇹', 'am-ET-MekdesNeural'],
    [sinhalaChars, 'si-LK', 'Sinhala (සිංහල)', '🇱🇰', 'si-LK-ThiliniNeural'],
    [khmerChars, 'km-KH', 'Khmer (ខ្មែរ)', '🇰🇭', 'km-KH-SreymomNeural'],
    [laoChars, 'lo-LA', 'Lao (ລາວ)', '🇱🇦', 'lo-LA-KeomanyNeural'],
    [myanmarChars, 'my-MM', 'Burmese (မြန်မာ)', '🇲🇲', 'my-MM-NilarNeural'],
  ];
  for (const [count, code, name, flag, suggestedVoiceId] of expandedScripts) {
    if (count >= 2 || (count > 0 && totalScriptChars === count)) {
      return {
        code,
        name,
        flag,
        isRomanizedDialect: false,
        suggestedVoiceId,
        confidence: Math.min(1.0, 0.7 + (count / trimmed.length) * 0.3),
      };
    }
  }

  // 2. Latin Script: Check for Romanized Indic Dialects (Manglish, Hinglish, Tanglish)
  const words = trimmed
    .toLowerCase()
    .replace(/[^\w\s']/g, ' ')
    .split(/\s+/)
    .filter((w) => w.length > 1);

  const COMMON_ENGLISH_WORDS = new Set([
    'the', 'be', 'to', 'of', 'and', 'a', 'in', 'that', 'have', 'i', 'it', 'for', 'not', 'on', 'with',
    'he', 'as', 'you', 'do', 'at', 'this', 'but', 'his', 'by', 'from', 'they', 'we', 'say', 'her', 'she',
    'or', 'an', 'will', 'my', 'one', 'all', 'would', 'there', 'their', 'what', 'so', 'up', 'out', 'if',
    'about', 'who', 'get', 'which', 'go', 'me', 'when', 'make', 'can', 'like', 'time', 'no', 'just', 'him',
    'know', 'take', 'people', 'into', 'year', 'your', 'good', 'some', 'could', 'them', 'see', 'other',
    'than', 'then', 'now', 'look', 'only', 'come', 'its', 'over', 'think', 'also', 'back', 'after', 'use',
    'two', 'how', 'our', 'work', 'first', 'well', 'way', 'even', 'new', 'want', 'because', 'any', 'these',
    'give', 'day', 'most', 'us', 'room', 'morning', 'sun', 'transforms', 'golden', 'paradise', 'whispered',
    'wonder', 'voice', 'eyes', 'watched', 'play', 'light', 'old', 'wooden', 'floorboards', 'sound', 'acoustic',
    'waves', 'travel', 'earth', 'density', 'normal', 'audio', 'test', 'sample', 'speech', 'speech-to-speech',
    'human', 'neural', 'quality', 'system', 'check', 'model', 'file', 'read', 'write', 'fast', 'faster',
    'rate', 'between', 'through', 'where', 'each', 'does', 'such', 'much', 'before', 'must', 'both', 'still',
    'here', 'there', 'why', 'should', 'again', 'off', 'went', 'never', 'found', 'thought', 'might', 'under',
    'always', 'something', 'without', 'place', 'different', 'around', 'small', 'every', 'great', 'tell', 'large',
    'water', 'long', 'very', 'another', 'world', 'high', 'last', 'start', 'run', 'number', 'part', 'little'
  ]);

  const matchedManglish: string[] = [];
  const matchedHinglish: string[] = [];
  const matchedTanglish: string[] = [];
  const matchedSpanish: string[] = [];
  const matchedFrench: string[] = [];
  const matchedGerman: string[] = [];
  let matchedEnglishCount = 0;

  for (const word of words) {
    if (COMMON_ENGLISH_WORDS.has(word)) matchedEnglishCount++;
    if (MANGLISH_KEYWORDS.has(word)) matchedManglish.push(word);
    if (HINGLISH_KEYWORDS.has(word)) matchedHinglish.push(word);
    if (TANGLISH_KEYWORDS.has(word)) matchedTanglish.push(word);
    if (SPANISH_KEYWORDS.has(word)) matchedSpanish.push(word);
    if (FRENCH_KEYWORDS.has(word)) matchedFrench.push(word);
    if (GERMAN_KEYWORDS.has(word)) matchedGerman.push(word);
  }

  // If text has English words and English count is higher or equal to dialect hits, retain English
  const totalDialectMatches = matchedHinglish.length + matchedManglish.length + matchedTanglish.length;
  const isPredominantlyEnglish = matchedEnglishCount > 0 && matchedEnglishCount >= totalDialectMatches;

  // Dialect detection requires at least 2 distinct keywords for 3+ word text, or >= 50% dialect ratio
  const minDialectThreshold = words.length >= 3 ? 2 : 1;

  // Manglish Detection
  if (!isPredominantlyEnglish && matchedManglish.length >= minDialectThreshold && matchedManglish.length >= matchedHinglish.length && matchedManglish.length >= matchedTanglish.length) {
    const confidence = Math.min(0.98, 0.65 + matchedManglish.length * 0.12);
    return {
      code: 'ml-Latn',
      name: 'Manglish (Malayalam Romanized)',
      flag: '🌴',
      isRomanizedDialect: true,
      nativeTargetCode: 'ml-IN',
      suggestedVoiceId: 'ml-IN-SobhanaNeural',
      confidence,
      sampleKeywordsFound: matchedManglish,
    };
  }

  // Hinglish Detection
  if (!isPredominantlyEnglish && matchedHinglish.length >= minDialectThreshold && matchedHinglish.length >= matchedTanglish.length) {
    const confidence = Math.min(0.98, 0.65 + matchedHinglish.length * 0.12);
    return {
      code: 'hi-Latn',
      name: 'Hinglish (Hindi Romanized)',
      flag: '🇮🇳',
      isRomanizedDialect: true,
      nativeTargetCode: 'hi-IN',
      suggestedVoiceId: 'hi-IN-SwaraNeural',
      confidence,
      sampleKeywordsFound: matchedHinglish,
    };
  }

  // Tanglish Detection
  if (!isPredominantlyEnglish && matchedTanglish.length >= minDialectThreshold) {
    const confidence = Math.min(0.98, 0.65 + matchedTanglish.length * 0.12);
    return {
      code: 'ta-Latn',
      name: 'Tanglish (Tamil Romanized)',
      flag: '🇮🇳',
      isRomanizedDialect: true,
      nativeTargetCode: 'ta-IN',
      suggestedVoiceId: 'ta-IN-PallaviNeural',
      confidence,
      sampleKeywordsFound: matchedTanglish,
    };
  }

  // European languages check
  if (matchedSpanish.length >= 2) {
    return {
      code: 'es-ES',
      name: 'Spanish (Español)',
      flag: '🇪🇸',
      isRomanizedDialect: false,
      suggestedVoiceId: 'es-ES-AlvaroNeural',
      confidence: 0.85,
    };
  }

  if (matchedFrench.length >= 2) {
    return {
      code: 'fr-FR',
      name: 'French (Français)',
      flag: '🇫🇷',
      isRomanizedDialect: false,
      suggestedVoiceId: 'fr-FR-DeniseNeural',
      confidence: 0.85,
    };
  }

  if (matchedGerman.length >= 2) {
    return {
      code: 'de-DE',
      name: 'German (Deutsch)',
      flag: '🇩🇪',
      isRomanizedDialect: false,
      suggestedVoiceId: 'de-DE-TanjaNeural',
      confidence: 0.85,
    };
  }

  // Default Latin: English
  return {
    code: 'en-US',
    name: 'English (US)',
    flag: '🇺🇸',
    isRomanizedDialect: false,
    suggestedVoiceId: 'en-US-AriaNeural',
    confidence: 0.9,
  };
}
