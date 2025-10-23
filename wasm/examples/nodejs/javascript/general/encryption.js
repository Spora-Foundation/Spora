const spora = require('../../../../nodejs/spora');

spora.initConsolePanicHook();

(async () => {

    let encrypted = spora.encryptXChaCha20Poly1305("my message", "my_password");
    console.log("encrypted:", encrypted);
    let decrypted = spora.decryptXChaCha20Poly1305(encrypted, "my_password");
    console.log("decrypted:", decrypted);

})();
