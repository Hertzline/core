const { invoke } = window.__TAURI__.core

const outEl = () => document.querySelector("#out")

const handleCorePing = async () => {
  const res = await invoke("core_ping", { client: "d1-tauri-ui" })
  outEl().textContent = JSON.stringify(res, null, 2)
}

const handleHelperPing = async () => {
  const res = await invoke("helper_ping")
  outEl().textContent = JSON.stringify(res, null, 2)
}

window.addEventListener("DOMContentLoaded", () => {
  document.querySelector("#ping-btn").addEventListener("click", (event) => {
    event.preventDefault()
    handleCorePing()
  })
  document.querySelector("#helper-btn").addEventListener("click", (event) => {
    event.preventDefault()
    handleHelperPing()
  })
})
