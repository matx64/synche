# Practical Example: Syncing a Project Between Two Devices

This example demonstrates how to synchronize a directory named `my-project` between two devices on the same local network: a laptop and a desktop.

## What You'll Need

- **Two devices** with Synche installed (Download a [Release](https://github.com/matx64/synche/releases/latest) or [Build from source](BUILD.md)).
- Both devices must be connected to the **same local network** (e.g., the same Wi-Fi network).

---

### Step 1: Configure the First Device (Laptop)

First, let's set up the shared directory on your laptop.

1.  **Run Synche** on your laptop.
2.  Open a web browser and navigate to the Synche Web GUI at **`http://localhost:42880`**.
3.  In the GUI, click the **"Add Directory"** button.
4.  Enter `my-project` as the directory name and save it.

Synche will create (if doesn't exist) a folder named `my-project` inside your Synche home directory (e.g., `~/Synche/my-project`).

> **Alternative Method: Editing `config.toml`**
>
> You can also add the directory by editing your `config.toml` file directly. This is useful for advanced configuration.
>
> **Laptop `config.toml`:**
> ```toml
> # This ID is unique to the laptop and is generated automatically.
> device_id = "laptop-unique-id"
> home_path = "/home/user/Synche" # Example path
>
> # Add this block to sync the "my-project" directory.
> [[directory]]
> name = "my-project"
> ```

### Step 2: Configure the Second Device (Desktop)

Now, repeat the process on your desktop. It is crucial that the directory name is **exactly the same** on both devices.

1.  **Run Synche** on your desktop.
2.  Open the Web GUI at **`http://localhost:42880`**.
3.  Click **"Add Directory"** and enter the same name: `my-project`.

Your desktop will now also have a `my-project` folder ready to sync.

### Step 3: Test the Synchronization

With Synche running on both devices, they will automatically discover each other on the network and begin to sync.

1.  On your **laptop**, navigate to the `my-project` directory and create a new file.
    -   Example location: `~/Synche/my-project/notes.txt`

2.  Within moments, Synche will detect the new file on your laptop and transfer it to your desktop. The file will appear in the corresponding directory on the desktop.
    -   Example location: `~/Synche/my-project/notes.txt`

Any changes, edits, or deletions made to files within the `my-project` directory on one device will now be automatically synchronized with the other.
