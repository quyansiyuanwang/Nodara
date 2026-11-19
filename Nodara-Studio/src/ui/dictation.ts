// Web Speech is optional in desktop webviews. Never start a microphone on load.
interface Recognition {
  lang: string;
  onresult: ((event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onerror: (() => void) | null;
  onend: (() => void) | null;
  start(): void;
  abort(): void;
}

const host = window as Window & {
  SpeechRecognition?: new () => Recognition;
  webkitSpeechRecognition?: new () => Recognition;
};
export const Dictation = host.SpeechRecognition ?? host.webkitSpeechRecognition;
