import { i18n } from "./index";

// Make sure the detector has finished before the tests assert translated text.
export default () => (i18n.isInitialized ? Promise.resolve() : i18n.init());
