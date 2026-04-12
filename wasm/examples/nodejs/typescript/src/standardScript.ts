import { payToAddressScript, addressFromScriptPublicKey, NetworkType } from "../../../../nodejs/spora"

// The supported path is to work with standard address scripts or ScriptRef + CKB-VM.
const address = "spora:qpamkvhgh0kzx50gwvvp5xs8ktmqutcy3dfs9dc3w7lm9rq0zs76vf959mmrp"
const scriptPublicKey = payToAddressScript(address)
const decoded = addressFromScriptPublicKey(scriptPublicKey, NetworkType.Mainnet)

console.log(scriptPublicKey)
console.log(decoded?.toString())
