# Configuring Pi with OpenRouter

[OpenRouter](https://openrouter.ai) gives you hundreds of models (Claude, GPT, Gemini, Qwen,
DeepSeek and more) with one key and one bill.

**What you need:** an OpenRouter API key, with credit on your OpenRouter account.

Pi is available to every user. Use an OpenRouter account or API
key you are authorized to use, with billing and model access covered by your agreement.
Read [Provider terms and permitted use](PROVIDER_TERMS.md).

## Steps

1. Create a key on [openrouter.ai/keys](https://openrouter.ai/keys) and add credit to your
   account.
2. In rightcommit open **Personal settings** → **Agents** → **Pi Agent**.
3. Set **Pi provider** to *OpenRouter*. The app loads the OpenRouter model list.
4. In **Model**, start typing to search, for example `claude` or `qwen`, and pick a model.
   IDs look like `vendor/model`. If the list does not load, type the ID by hand (copy it from
   the model page on openrouter.ai).
5. Paste the key in **Personal API key** and press **Save**. The key is saved for OpenRouter
   only: keys you saved for other Pi providers are kept, and selecting OpenRouter again later
   reuses this key without retyping it. The bin icon removes only the OpenRouter key.

Pick a model that supports **tool calling**: Pi needs tools to read and edit files.

The **Model** field of an Agent node that runs Pi with OpenRouter offers the same list. Leave
it empty to use the model from Personal settings, or type any ID by hand.
The **Supervisor model** field of the execution mode (workflow execution mode, run start and
scheduled runs) offers it too when the supervisor runs Pi with OpenRouter: **Supervisor agent**
set to *Pi Agent*, or *Personal preference* with Pi as your Default agent. The supervisor uses
the Pi provider from Personal settings (the default provider in Multi-provider mode). Leave
the field empty to use the model from Personal settings.

The execution mode remembers which Pi provider the supervisor model was chosen for. If you later
switch the Pi provider in Personal settings (or your Default agent is no longer Pi), a supervised
run with that model does not start: starting it by hand shows an error that names the
**Supervisor model** field, a scheduled run is recorded as an error in the schedule history with
the schedule's name, and the execution mode and the schedule show a warning. Choose the model
again for the current provider, or leave the field empty. Schedules and workflows saved by
earlier versions start as before.

## Other ways to give the key

- **Installation-wide:** set `OPENROUTER_API_KEY` in `.env` (Docker or Podman, then
  `docker compose up -d`) or in the shell before starting `rightcommit`. It is used by accounts
  without a personal key.
- **Administrator key:** an OpenRouter key saved by the administrator in Pi settings is
  used by users without a personal OpenRouter key, while the administrator has OpenRouter
  selected in single-provider mode.

## Good to know

- **No `web_search`:** OpenRouter has no built-in web search, so Pi does not get the
  `web_search` tool. `web_fetch` still works, so the agent can read pages you name.
- **Limits:** OpenRouter applies the guardrails and provider restrictions set on your key.
- **Costs:** the FinOps panel uses OpenRouter's public prices for its estimates.
