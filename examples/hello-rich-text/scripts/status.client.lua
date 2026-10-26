-- Sets rich text at runtime: Status has RichText on, so the tags style it.
local status = workspace.Hud.Status

task.wait(1)
status.Text = 'The door is <font color="#55dd55"><b>open</b></font>'
