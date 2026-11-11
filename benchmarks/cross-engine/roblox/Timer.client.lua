-- XB10k: the fixed camera, and the timing. Waits for the server's build and
-- for every part to arrive, then 60 warm-up frames, then times 600 frames
-- between RenderStepped calls (the start of each rendered frame).
local RunService = game:GetService("RunService")
local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
camera.FieldOfView = 70
camera.CFrame = CFrame.lookAt(Vector3.new(-260, 180, -260), Vector3.new(0, 0, 0))

while not workspace:GetAttribute("XB10kReady") do
    task.wait()
end
local function arrived()
    local grid, spinners, drops = workspace:FindFirstChild("Grid"), workspace:FindFirstChild("Spinners"), workspace:FindFirstChild("Drops")
    return grid and spinners and drops
        and #grid:GetChildren() == 8899 and #spinners:GetChildren() == 100 and #drops:GetChildren() == 1000
end
while not arrived() do
    task.wait()
end

local WARMUP, MEASURED = 60, 600
local samples = table.create(MEASURED)
local frame, last, firstFrameMs = 0, os.clock(), 0
local connection
connection = RunService.RenderStepped:Connect(function()
    local now = os.clock()
    local ms = (now - last) * 1000
    last = now
    frame += 1
    if frame == 1 then
        firstFrameMs = ms
    elseif frame > WARMUP + 1 then
        table.insert(samples, ms)
        if #samples == MEASURED then
            connection:Disconnect()
            table.sort(samples)
            local sum = 0
            for _, s in samples do
                sum += s
            end
            local median = (samples[MEASURED / 2] + samples[MEASURED / 2 + 1]) / 2
            print("XB10K_RESULT " .. game:GetService("HttpService"):JSONEncode({
                engine = "Roblox",
                version = version(),
                renderer = "Studio Play",
                frames = MEASURED,
                median_ms = median,
                mean_ms = sum / MEASURED,
                p95_ms = samples[math.ceil(MEASURED * 0.95)],
                worst_ms = samples[MEASURED],
                fps_from_median = 1000 / median,
                build_ms = workspace:GetAttribute("XB10kBuildMs"),
                first_frame_ms = firstFrameMs,
                fixed_timestep = false,
            }))
        end
    end
end)
