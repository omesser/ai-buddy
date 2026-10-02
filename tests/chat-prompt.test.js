// DROPPED (#1268): "nothing in the Chat surface saves on a keystroke"
// parses addEventListener names out of chat.js. It does not type or save.
// DROPPED (#1268): "the save is asked for twice before the conversation is spent"
// compares source indexes of promptConfirm and invoke(chat_prompt).
// DROPPED (#1268): "the tab warns that saving starts a new conversation before it offers to"
// compares html indexes of the warning sentence and the save button.
// DROPPED (#1268): "Prompt tab actions sit after the authored layers, outside the scroll"
// matches .prompt-actions rules in CSS.
// DROPPED (#1268): "the tab shows the three concatenated layers, app then Character then instance"
// matches layer headings in the prompt tab html.
// DROPPED (#1268): "every empty Prompt tab layer says Empty"
// matches the word Empty in the prompt tab html.
// DROPPED (#1268): "an empty Instance Prompt field keeps rows and matches the Personality block"
// matches rows and a class name in the prompt field markup.
// DROPPED (#1268): "the save warning sits outside the Instance Prompt field"
// compares html indexes of the warning and the textarea.
// DROPPED (#1268): "Prompt tab Save and Discard stay disabled until the field is dirty"
// matches disabled and promptDirty in chat.js source.
// DROPPED (#1268): "Discard restores the last saved text; Cancel only aborts confirm"
// matches Discard and Cancel handler bodies in chat.js.
