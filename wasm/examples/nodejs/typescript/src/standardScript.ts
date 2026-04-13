import { payToAddressLockScript, addressFromLockScript, NetworkType } from "../../../../nodejs/spora"

// The supported path is to work with standard address scripts or ScriptRef + CKB-VM.
const address = "spora:qpamkvhgh0kzx50gwvvp5xs8ktmqutcy3dfs9dc3w7lm9rq0zs76vf959mmrp"
const lockScript = payToAddressLockScript(address)
const decoded = addressFromLockScript(lockScript, NetworkType.Mainnet)

console.log(lockScript)
console.log(decoded?.toString())
