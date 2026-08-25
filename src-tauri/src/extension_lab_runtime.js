// [GRAIN] Recommendation Lab worker. This is copied into ordinary unpacked
// projects; it has no Tauri access and runs through the production worker API.
(function () {
  var extensionId = "__GRAIN_LAB_EXTENSION_ID__";
  var configs = {
    "com.grain.lab.stream-music": {
      name: "Stream Music",
      mode: "confirm",
      commands: [
        "play",
        "pause",
        "resume",
        "next",
        "previous",
        "shuffle",
        "repeat",
        "queue",
        "volume-up",
        "volume-down",
        "like",
        "unlike",
        "playlist",
        "artist",
        "album",
        "device",
      ],
    },
    "com.grain.lab.music-library": {
      name: "Music Library",
      mode: "confirm",
      commands: [
        "play-local",
        "shuffle-library",
        "recent",
        "add",
        "remove",
        "playlist",
        "artist",
        "album",
        "download",
        "metadata",
        "queue",
        "search",
      ],
    },
    "com.grain.lab.issue-tracker": {
      name: "Issue Tracker",
      mode: "issue-form",
      commands: [
        "create",
        "update",
        "assign",
        "unassign",
        "prioritize",
        "label",
        "move",
        "close",
        "reopen",
        "comment",
        "link",
        "duplicate",
        "block",
        "unblock",
        "estimate",
        "schedule",
        "search",
        "list",
        "triage",
        "archive",
      ],
    },
    "com.grain.lab.code-host": {
      name: "Code Host",
      mode: "confirm",
      commands: [
        "pull-request",
        "merge",
        "review",
        "approve",
        "request-changes",
        "issue",
        "branch",
        "tag",
        "release",
        "checks",
        "workflow",
        "fork",
        "clone",
        "compare",
        "blame",
        "commit",
        "milestone",
        "project",
        "discussion",
        "search",
      ],
    },
    "com.grain.lab.translator": {
      name: "Translator - Multilingual",
      mode: "direct",
      commands: ["translate", "detect-language", "romanize", "define"],
    },
    "com.grain.lab.calendar": {
      name: "Calendar Planner",
      mode: "calendar-form",
      commands: [
        "create",
        "move",
        "cancel",
        "availability",
        "agenda",
        "invite",
        "decline",
        "accept",
        "remind",
        "search",
        "focus-time",
      ],
    },
    "com.grain.lab.team-chat": {
      name: "Team Chat",
      mode: "message-form",
      commands: [
        "channel-message",
        "direct-message",
        "reply",
        "edit",
        "delete",
        "react",
        "pin",
        "unpin",
        "save",
        "search",
        "unread",
        "status",
        "remind",
      ],
    },
    "com.grain.lab.email": {
      name: "Email Assistant",
      mode: "message-form",
      commands: [
        "compose",
        "reply",
        "reply-all",
        "forward",
        "archive",
        "delete",
        "label",
        "star",
        "snooze",
        "search",
        "draft",
        "schedule",
        "unsubscribe",
        "summarize",
      ],
    },
    "com.grain.lab.meeting-notes": {
      name: "Meeting Notes",
      mode: "long-result",
      commands: [
        "minutes",
        "decisions",
        "actions",
        "summary",
        "attendees",
        "risks",
        "follow-ups",
      ],
    },
    "com.grain.lab.document-notes": {
      name: "Document Notes",
      mode: "long-result",
      commands: [
        "create",
        "append",
        "find",
        "move",
        "tag",
        "link",
        "archive",
        "outline",
        "export",
      ],
    },
    "com.grain.lab.task-list": {
      name: "Task List",
      mode: "task-form",
      commands: [
        "create",
        "complete",
        "reopen",
        "schedule",
        "prioritize",
        "assign",
        "label",
        "move",
        "list",
        "search",
      ],
    },
    "com.grain.lab.knowledge-base": {
      name: "Knowledge Base Research and Documentation Assistant",
      mode: "long-result",
      commands: [
        "search",
        "summarize",
        "open",
        "cite",
        "compare",
        "recent",
        "owner",
        "related",
      ],
    },
    "com.grain.lab.weather": {
      name: "Weather",
      mode: "direct",
      commands: ["current", "hourly", "daily", "rain", "wind"],
    },
    "com.grain.lab.maps": {
      name: "Maps and Directions",
      mode: "confirm",
      commands: [
        "navigate",
        "route",
        "nearby",
        "traffic",
        "transit",
        "walk",
        "cycle",
        "save-place",
        "share-place",
      ],
    },
    "com.grain.lab.contacts": {
      name: "Contacts",
      mode: "contact-form",
      commands: [
        "find",
        "create",
        "update",
        "merge",
        "delete",
        "company",
        "favorites",
      ],
    },
    "com.grain.lab.calculator": {
      name: "Calculator",
      mode: "direct",
      commands: [
        "arithmetic",
        "percent",
        "unit",
        "currency",
        "split",
        "interest",
        "date",
        "statistics",
      ],
    },
    "com.grain.lab.browser-search": {
      name: "Web Search",
      mode: "direct",
      commands: ["search", "news", "images", "shopping", "academic"],
    },
    "com.grain.lab.file-organizer": {
      name: "File Organizer",
      mode: "danger",
      commands: [
        "find",
        "rename",
        "move",
        "copy",
        "delete",
        "organize",
        "compress",
        "extract",
        "duplicate",
        "recent",
        "large",
        "type",
        "tag",
        "open",
        "reveal",
      ],
    },
    "com.grain.lab.timer": {
      name: "Timer",
      mode: "completion",
      commands: ["start", "pause", "resume", "cancel"],
    },
    "com.grain.lab.smart-home": {
      name: "Smart Home",
      mode: "danger",
      commands: [
        "light-on",
        "light-off",
        "brightness",
        "temperature",
        "scene",
        "lock",
        "unlock",
        "garage",
        "fan",
        "camera",
        "alarm",
        "music",
        "vacuum",
        "blinds",
        "sprinkler",
        "status",
        "energy",
        "away",
      ],
    },
    "com.grain.lab.video-meeting": {
      name: "Video Meeting",
      mode: "confirm",
      commands: [
        "start",
        "join",
        "invite",
        "link",
        "mute",
        "camera",
        "share",
        "record",
      ],
    },
    "com.grain.lab.clipboard-tools": {
      name: "Clipboard Tools",
      mode: "direct",
      commands: ["plain-text", "markdown", "list", "clean", "case", "inspect"],
    },
    "com.grain.lab.expense-tracker": {
      name: "Expense Tracker",
      mode: "expense-form",
      commands: [
        "record",
        "categorize",
        "receipt",
        "reimburse",
        "split",
        "budget",
        "weekly",
        "monthly",
        "merchant",
        "search",
        "export",
        "recurring",
      ],
    },
    "com.grain.lab.travel-planner": {
      name: "Travel Planner",
      mode: "travel-form",
      commands: [
        "itinerary",
        "flight",
        "train",
        "hotel",
        "restaurant",
        "activity",
        "route",
        "budget",
        "packing",
        "visa",
        "weather",
        "calendar",
        "share",
        "compare",
      ],
    },
  };

  var active = null;

  function phrases(id) {
    return String(id)
      .split("-")
      .concat([String(id).replace(/-/g, " ")]);
  }

  async function chooseCommand(request, config) {
    var candidates = config.commands.map(function (id) {
      return { id: id, phrases: phrases(id) };
    });
    var ranked = await grain.match.lexical(request, candidates);
    if (ranked && ranked.length) return ranked[0].id;
    return config.commands[0];
  }

  function metadata(request, command) {
    return [
      { type: "metadata", label: "Resolved command", value: command },
      {
        type: "metadata",
        label: "Original request",
        value: request.slice(0, 2048),
      },
    ];
  }

  function confirmView(config, request, command, danger) {
    return {
      version: 1,
      title: danger ? "Review consequential action" : "Confirm request",
      description:
        config.name +
        " resolved one of " +
        config.commands.length +
        " internal commands.",
      root: {
        type: "stack",
        gap: "md",
        children: [
          {
            type: "section",
            title: config.name,
            children: metadata(request, command),
          },
          {
            type: "text",
            tone: danger ? "warning" : "muted",
            text: danger
              ? "This lab action is drawn with Grain's destructive-action treatment. It does not touch real data."
              : "The fixture waits for explicit confirmation before returning its result.",
          },
        ],
      },
      actions: [
        {
          id: "confirm",
          label: danger ? "Run test action" : "Confirm",
          intent: danger ? "danger" : "primary",
          kind: "submit",
        },
        { id: "cancel", label: "Cancel", intent: "cancel", kind: "cancel" },
      ],
    };
  }

  function formView(config, request, command) {
    var children = metadata(request, command);
    if (config.mode === "issue-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "title",
          label: "Issue title",
          value: request.slice(0, 120),
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "details",
          label: "Description",
          value: request,
          rows: 5,
          maxLength: 2000,
        },
        {
          type: "select",
          id: "priority",
          label: "Priority",
          value: "medium",
          options: [
            { value: "low", label: "Low" },
            { value: "medium", label: "Medium" },
            { value: "high", label: "High" },
            { value: "urgent", label: "Urgent" },
          ],
        },
        {
          type: "checkbox",
          id: "notify",
          label: "Notify the project team",
          checked: true,
        },
      ]);
    } else if (config.mode === "calendar-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "title",
          label: "Event title",
          value: request.slice(0, 100),
          required: true,
          maxLength: 100,
        },
        {
          type: "text_field",
          id: "when",
          label: "Date and time",
          value: "Tomorrow at 10:00",
          required: true,
          maxLength: 80,
        },
        {
          type: "text_area",
          id: "attendees",
          label: "Attendees",
          placeholder: "Names or email addresses",
          rows: 3,
          maxLength: 500,
        },
      ]);
    } else if (config.mode === "message-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "recipient",
          label: "Recipient",
          placeholder: "Person, channel, or address",
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "message",
          label: "Message",
          value: request,
          rows: 6,
          required: true,
          maxLength: 2000,
        },
      ]);
    } else if (config.mode === "task-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "task",
          label: "Task",
          value: request.slice(0, 160),
          required: true,
          maxLength: 160,
        },
        {
          type: "select",
          id: "priority",
          label: "Priority",
          value: "normal",
          options: [
            { value: "normal", label: "Normal" },
            { value: "high", label: "High" },
          ],
        },
      ]);
    } else if (config.mode === "contact-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "name",
          label: "Name",
          required: true,
          maxLength: 120,
        },
        { type: "text_field", id: "email", label: "Email", maxLength: 200 },
        { type: "text_field", id: "phone", label: "Phone", maxLength: 80 },
      ]);
    } else if (config.mode === "expense-form") {
      children = children.concat([
        {
          type: "text_field",
          id: "amount",
          label: "Amount",
          placeholder: "0.00",
          required: true,
          maxLength: 32,
        },
        {
          type: "select",
          id: "category",
          label: "Category",
          value: "meals",
          options: [
            { value: "meals", label: "Meals" },
            { value: "travel", label: "Travel" },
            { value: "software", label: "Software" },
            { value: "other", label: "Other" },
          ],
        },
        {
          type: "checkbox",
          id: "reimbursable",
          label: "Reimbursable",
          checked: false,
        },
      ]);
    } else {
      children = children.concat([
        {
          type: "text_field",
          id: "destination",
          label: "Destination",
          required: true,
          maxLength: 120,
        },
        {
          type: "text_area",
          id: "preferences",
          label: "Preferences",
          value: request,
          rows: 5,
          maxLength: 1600,
        },
      ]);
    }
    return {
      version: 1,
      title: config.name,
      description:
        "Editable Grain-rendered form from a Recommendation Lab extension.",
      root: {
        type: "stack",
        gap: "md",
        children: [
          { type: "section", title: "Request details", children: children },
        ],
      },
      actions: [
        {
          id: "submit",
          label: "Complete test",
          intent: "primary",
          kind: "submit",
        },
        { id: "cancel", label: "Cancel", intent: "cancel", kind: "cancel" },
      ],
    };
  }

  grain.ui.onEvent(function (event) {
    if (!active) return;
    if (event.kind === "cancel") {
      active = null;
      return;
    }
    if (event.kind === "change") return;
    var values = event.values || {};
    var fields = Object.keys(values)
      .map(function (key) {
        return key + "=" + String(values[key]);
      })
      .join(", ");
    var message =
      active.config.name +
      " completed '" +
      active.command +
      "' in the Recommendation Lab.";
    if (fields) message += "\n\nSubmitted values: " + fields;
    active = null;
    return { message: message };
  });

  grain.onRequest(async function (request) {
    active = null;
    var config = configs[extensionId];
    if (!config) return { error: "Recommendation Lab profile is missing" };
    var lower = request.toLowerCase();
    if (
      extensionId === "com.grain.lab.travel-planner" &&
      (lower.indexOf("navigate") >= 0 || lower.indexOf("directions") >= 0)
    ) {
      return {
        decline:
          "This is a point-to-point route; Maps and Directions is a better owner.",
      };
    }
    if (
      extensionId === "com.grain.lab.file-organizer" &&
      lower.indexOf("protected") >= 0
    ) {
      return {
        error:
          "The lab refused a protected-file operation to exercise the terminal failure surface.",
      };
    }
    var command = await chooseCommand(request, config);
    if (config.mode === "direct") {
      return {
        message:
          config.name +
          " handled '" +
          command +
          "' directly, without a confirmation surface.\n\nOriginal request: " +
          request,
      };
    }
    if (config.mode === "long-result") {
      return {
        message:
          config.name +
          " produced a finite result for '" +
          command +
          "'.\n\nThis deliberately longer second paragraph exercises wrapping, Copy, Insert, and Replace without opening an extension-owned conversation. The original request remains visible here for verification: " +
          request,
      };
    }
    if (config.mode === "completion") return {};
    active = { config: config, request: request, command: command };
    if (config.mode === "confirm")
      return { view: confirmView(config, request, command, false) };
    if (config.mode === "danger")
      return { view: confirmView(config, request, command, true) };
    return { view: formView(config, request, command) };
  });
})();
