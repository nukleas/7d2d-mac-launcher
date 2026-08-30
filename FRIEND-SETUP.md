# Undead Legacy on Mac — simple setup

**You need:** a Mac (Apple Silicon), Steam, and *7 Days to Die*.  
**You don’t need:** Terminal, coding, or shell scripts.

---

## 1) Pick your Undead Legacy

There are two versions, and they need **different versions of the game**:

| You want | Pick | Steam branch you'll need |
|----------|------|--------------------------|
| To play on our server | **Undead Legacy 2.7** | `v2.6` |
| To keep an old A20.7 save | **Undead Legacy 2.6** | `alpha20.7` |

**Both are Steam *beta* branches.** Neither is the normal/default version —
Steam's default is V3.2, and Undead Legacy does not work on it.

If you're new, choose **2.7**. It's the version being actively worked on.
Heads-up: it updates often, and a new build can require starting a new game.

**2.7 is a big download — about 7 GB, and you need ~14 GB free** to download and
unpack it. It can take an hour or more. Leave the launcher open. If the download
drops, just press Install again — it keeps whatever finished and carries on.

The launcher installs one exact version so everyone matches the server. It shows
the build number when it finishes — if you can't join, that's the first thing to
compare.

---

## 2) Steam (once)

1. Open **Steam**
2. Right-click **7 Days to Die** → **Properties** → **Betas**
3. Set the branch the launcher asks for (see the table above)
4. Wait until Steam finishes updating

---

## 3) Install this helper

### If you got a zip folder

1. Unzip it
2. **Double-click `Open Me First`**  
   (fixes the common Sequoia “app is damaged” warning and puts the launcher in Applications)
3. If Mac asks, click **Open** / **Allow**

### If Mac still says “damaged”

1. **System Settings** → **Privacy & Security**
2. Scroll down → **Open Anyway** if shown  
3. Or run **Open Me First** again

---

## 4) Install the mod

1. Open **7D2D Mac Launcher**
2. Choose your version at the top of the install panel
3. Wait for the green status (game found + the right game version)
4. Click **Install Undead Legacy** — wait (progress bar; several minutes is OK)
5. Click **Play Undead Legacy**

**Always use this app’s Play button** — not Steam’s Play — so the mod loads correctly.

---

## Playing later

Open **7D2D Mac Launcher** from Applications → **Play**.

---

## Tips

- Start a **new** random world the first time  
- First load can be slow — wait  
- If Play fails, click **Install** once more (repairs), then Play  

---

## Troubleshooting

| What you see | What to try |
|--------------|-------------|
| Game not found | Install 7DTD in Steam, then **Check again** |
| Needs a different game version | Steam → Betas → set the branch it names, wait, **Check again** |
| Install failed | Free disk space (2.7 needs ~14 GB), **Install** again |
| Can't join the server | Check your build number matches the server's |
| Download keeps dropping | Press **Install** again — finished parts are kept, so it resumes where it got to |
| “Not enough free space” | It tells you how much you need — clear that much, then **Install** |
| Install incomplete | **Install** again (repair) |
| App is damaged | Use **Open Me First**, or Privacy & Security → Open Anyway |
| Red errors / broken menu | You used Steam Play — quit and use this app’s **Play** |

Still stuck? Send a screenshot of the launcher window.

---

## Cheat sheet

1. Pick **2.7** (our server) or **2.6** (old A20.7 saves)  
2. Set the Steam **beta** branch the app names — not the default  
3. **Open Me First** (once)  
4. **Install Undead Legacy**  
5. **Play** (from this app every time)  

That’s it.

---

# On Windows?

**This launcher is Mac-only** — it exists because the standard mod launcher was
broken on Mac. On Windows the normal tools work fine, so you don't need it.

Two options, both fine:

- **SphereII's 7D2D Mod Launcher** — the mainstream Windows tool:
  <https://github.com/SphereII/7D2DModLauncher/releases/latest>
- **By hand** — five minutes, steps below.

## By hand (Windows)

**1) Set the Steam beta**

Steam → right-click *7 Days to Die* → **Properties** → **Betas** → **`v2.6`**.

Not the default (that's V3.2), not `alpha20.7`. Wait for Steam to finish.

**2) Find the game folder**

Steam → right-click the game → **Manage** → **Browse local files**.

**3) Delete the vanilla `Mods` folder**

The stock game ships `Mods\0_TFP_Harmony`, which **breaks Undead Legacy**. Delete
the whole `Mods` folder (or rename it to `Mods_off` if you want it back later).

*To go back to vanilla later: Steam → Properties → Installed Files → Verify
integrity of game files.*

**4) Unpack Undead Legacy into the game folder**

Get the **two-part** experimental download from <https://ul.subquake.com/download>
(about 7 GB total; you need roughly 14 GB free). Unpack **both parts into the
same place** — the game folder — so they merge.

When you're done, these sit directly beside `7DaysToDie.exe`:

```
BepInEx\
Mods\
doorstop_libs\
doorstop_config.ini
winhttp.dll
```

Both parts write into `Mods\UndeadLegacy\`, so let them merge — don't replace one
folder with the other or you'll lose half the mod.

> **The build must match the server exactly.** 7 Days to Die refuses connections
> between different mod builds, and Undead Legacy 2.7 publishes a new build every
> few days. Ask which build the server runs and install that one. **Do not let it
> auto-update.**

**5) Turn EasyAntiCheat off**

Steam → right-click the game → **Properties** → **General** → **Launch Options**,
and enter:

```
-noeac
```

The mod cannot load with EAC on. Nothing else is needed to make the mod
load — Windows picks up `winhttp.dll` automatically, so there are no environment
variables or scripts to set.

**6) Play**

Launch from Steam. **First start is slow** — several minutes is normal. Then
**Join Game → Connect to IP** and enter the address you were given.

## Windows troubleshooting

| What you see | What to try |
|--------------|-------------|
| Game loads but looks vanilla | `winhttp.dll` isn't beside `7DaysToDie.exe`, or EAC is still on |
| Red error text / broken menus | EAC is on — add `-noeac` to Launch Options |
| Rejected when joining | Your mod build doesn't match the server's |
| Crash on start | Vanilla `Mods\0_TFP_Harmony` is still there — delete `Mods` and unpack again |
