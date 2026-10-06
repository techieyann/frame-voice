# Settings by tab

Open the microphone icon in the Desktop tray and choose **Configure…** to open
**Frame Voice Configuration**.

You can make changes across all three tabs, then click **Save settings** once in
the shared footer. Save is disabled until something changes. Saving applies the
settings and restarts dictation, so finish any recording first.

## General

| Setting | What it does |
| --- | --- |
| **Text delivery** | Typing sends keystrokes; Paste uses the clipboard in compatible standalone apps. Desktop uses typing automatically. |
| **Start and stop tones** | Enables or disables start, stop, and cancel feedback sounds. |
| **Separator after dictation** | Space keeps consecutive dictations from running together. None adds no separator. |
| **Maximum recording seconds** | Limits each recording; the default is 30 seconds. |

The tray shows a blue microphone while dictation runs and a red crossed-out
microphone when paused. Use the tray's start/resume or pause options to control it.
In the headset, the small microphone waits for audio startup, then grows toward
the start tone. Wait for the tone before speaking. Release the joystick cap to
finish; click it during recording to cancel.

### Updates

The **Updates** group shows your installed version. Development builds also show
a build revision so you can distinguish versions tested between releases.

Click **Check for updates** to compare it with the latest stable release. Internet
access is required. Checks run only when you request them.

If an update is available, save any pending settings, then choose **Update…** to
open the installer. Updates retain saved settings, API keys, and downloaded models.
**Open releases** lets you read the release notes. Experimental releases are not
selected by this check, and newer installed versions are not downgraded.

### Uninstall

**Uninstall…** is at the bottom of General. It opens a confirmation dialog with
both retention options checked by default:

| Confirmation option | Checked | Unchecked |
| --- | --- | --- |
| **Keep current configuration** | Keeps saved settings, API keys, and your managed custom prompt. | Removes them. |
| **Keep downloaded models** | Keeps Frame Voice's model downloads for later reuse. | Deletes them to recover storage. |

Uncheck both to remove Frame Voice's settings, saved keys, downloaded models,
and app data. Installation backups are removed during uninstall. Separately
installed voice tools and their models are left alone.

## Transcription

### Transcription path

| Transcription option | Speed | Internet needed | Model download |
| --- | --- | --- | --- |
| **Groq · Cloud** | Fastest | Every dictation | None |
| **Local · Fast** | Fast | Initial download only | About 142 MiB |
| **Local · Balanced** | Slow | Initial download only | About 466 MiB |

Groq requires a [Groq API key](https://console.groq.com/keys). The local options
process speech on your Frame; the larger model favors accuracy.

Select a path, then **Save settings**. Local transcription works offline after
its initial download. Switching to local retains your saved Groq API key so you
can switch back later. Existing verified model downloads are reused.

If downloading or preparing a new model fails, the current transcription path
is retained.

### Language

Choose a language from the searchable list, or **Automatic detection** when you
use more than one language. English local transcription uses an English model;
other languages and automatic detection use a multilingual model. Changing this
selection may require another download.

For accented characters and non-Latin scripts, use a clipboard-compatible
standalone app such as Firefox. Nested Desktop typing currently supports ASCII.
Controller buttons work independently of the transcription language; spoken
submit/clear command aliases remain English.

### Groq options

These controls appear when **Groq · Cloud** is selected:

| Setting | What you need to know |
| --- | --- |
| **Groq API key** | Paste your key here. **Get a Groq API key** opens Groq's key page. The key stays saved when switching to local transcription. |
| **Clean up transcript** | Removes fillers and stutters, resolves self-corrections, and adds punctuation using Groq. |
| **In-headset notifications** | Controls Groq usage and limit notifications. |
| **Convert spoken slash to /** | Optionally turns a spoken “slash” into `/` during cleanup. It is off by default. |

**Edit cleanup prompt…** opens the built-in editor. Customize the instructions,
click **Apply**, then **Save settings**. Apply updates the draft; Save writes it.
**Reset to default** restores the shipped prompt in the editor. Apply and Save
that draft to keep the reset. Cancel discards edits made inside the editor.

The [default prompt](../assets/groq-prompt.txt) asks for neutral cleanup while
preserving meaning, language, names, and technical text. Your custom prompt stays
on your Frame and is preserved by updates.

Quiet recordings and uncertain Groq results may cancel instead of inserting
text. This reduces invented transcripts from background sounds.

### Downloaded models and storage

The **Downloaded models** section shows the stored models and their sizes.
English and multilingual copies appear separately.

To free space without uninstalling, click **Delete…** beside an unused model and
confirm. The active model is protected: select and save another transcription
path or model first. Choosing a deleted model later downloads it again.

## Controllers

Enable dictation for each hand and choose the submit and clear buttons. Button
names are shown as **A**, **B**, **X**, **Y**, or **D-pad Up/Down/Left/Right**.
Choose **None** to disable a particular action. Save to apply new bindings.

| Default control | Action |
| --- | --- |
| **Either joystick cap** | Touch briefly, release, then touch and hold to record. |
| **Joystick click during recording** | Cancel the recording. |
| **Right A / left D-pad Right** | Submit after text appears. |
| **Right B / left D-pad Left** | Clear after text appears, when the button is released. |

Submit and clear are available for ten seconds after successful dictation and
stop when focus changes. A plain joystick hold does not start recording.

### B's Desktop space

In Steam's default Desktop layout, B also types a space. Leave **B also types a
space in Desktop** enabled to include that extra character when clearing.
Disable it if your custom Steam B mapping does not type a space. This adjustment
applies to right-hand B in Desktop; spoken Clear and other bindings keep their
own behavior.

Desktop Clear tracks the text inserted by the current Frame Voice process,
including its separator. Restarting dictation resets that history. Test Clear
with a fresh dictation after an update or settings save. Moving the cursor or
editing the text yourself can also invalidate the tracked count.

## More detail

See [installation and removal](INSTALLATION.md) for installer and rollback
instructions, or the [configuration reference](CONFIGURATION.md) for terminal
commands, advanced settings, and troubleshooting.
