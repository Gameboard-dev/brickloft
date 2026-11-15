-- Places and removes the bricks the omegga-arcs plugin sends.
--
-- The plugin never sends Lua: it calls the two handlers below with plain data
-- (emitPlugin('call', ...) on the ue4ss plugin), and everything in that data
-- is checked here before it reaches the game. Building Lua text from brick
-- lists and running it through `exec` would also work, and would make every
-- number and name in a build a way to inject code into the game process.
--
-- Bricks go in through a transaction rather than BR.brick.place: the ue4ss
-- docs measure about 70 ms per brick for place against 2.7 ms per hundred in
-- a transaction, and a transaction reports how many bricks were blocked.
-- omegga-ue4ss/docs/lua-api.md, "Components, gates and wires"
local Omegga = require("omegga")
local BR = require("omegga.brickadia")

-- the library's functions under their documented names, or the flat names
-- its interop example uses
local B = BR.brick or {}
local make_builder = B.builder or BR.builder
local transaction = B.transaction or BR.transaction
local server_player = (BR.player and type(BR.player) == "table" and BR.player.server) or BR.player
local MATERIALS = B.MATERIAL or {}

local ASSETS = {
    PB_DefaultMicroBrick = true,
    PB_DefaultMicroWedge = true,
    PB_DefaultMicroWedgeTriangleCorner = true,
    PB_DefaultMicroWedgeOuterCorner = true,
    PB_DefaultMicroWedgeInnerCorner = true,
    PB_DefaultMicroWedgeCorner = true,
}
local MAX_BRICKS_PER_CALL = 4096
local FIELDS = 9 -- asset index, size x y z, position x y z, orientation, colour

-- builds[player] is a stack of builds, each a list of brick ids, newest last
local builds = {}

local function int(v, lo, hi, what)
    if math.type(v) ~= "integer" then
        -- JSON numbers can arrive as floats; accept whole ones only
        if type(v) ~= "number" or v ~= math.floor(v) then error(what .. " is not a whole number", 0) end
        v = math.floor(v)
    end
    if v < lo or v > hi then error(what .. " " .. v .. " is outside " .. lo .. ".." .. hi, 0) end
    return v
end

local function owner_of(name)
    -- the player's own bricks if they can be found, so they can edit them
    local find = BR.player and type(BR.player) == "table" and BR.player.find
    local ps = find and find(name)
    local info = ps and BR.player.info(ps)
    if info and math.type(info.owner) == "integer" then return info.owner end
    local own = (BR.player and type(BR.player) == "table" and BR.player.owner_index) or BR.owner_index
    return own()
end

Omegga.handle("arcs_ready", function() return BR.ready() end)

-- req = { player, first, origin = {x, y, z}, color = {r, g, b}, material,
--         palette = {{r, g, b}, ...}, assets = {name, ...},
--         bricks = {flat integers, FIELDS per brick} }
-- A brick's colour is 0 for `color` (the player's own) or an index into `palette`.
-- `first` starts a new build; later calls with first = false add to it, so a
-- large build arrives in several calls and is still undone as one.
Omegga.handle("arcs_place", function(req)
    assert(BR.ready(), "the world is not loaded yet")
    assert(type(req) == "table" and type(req.player) == "string" and #req.player <= 64, "bad request")
    assert(type(req.bricks) == "table" and #req.bricks % FIELDS == 0, "bad brick list")
    local count = #req.bricks // FIELDS
    assert(count <= MAX_BRICKS_PER_CALL, "too many bricks in one call")
    assert(type(req.assets) == "table" and #req.assets >= 1 and #req.assets <= 8, "bad asset list")
    for _, name in ipairs(req.assets) do
        assert(ASSETS[name], "asset not allowed: " .. tostring(name))
    end
    local ox = int(req.origin[1], -1000000, 1000000, "origin x")
    local oy = int(req.origin[2], -1000000, 1000000, "origin y")
    local oz = int(req.origin[3], -1000000, 1000000, "origin z")
    local function rgb(c, what)
        assert(type(c) == "table", "bad " .. what)
        return { int(c[1], 0, 255, "red"), int(c[2], 0, 255, "green"), int(c[3], 0, 255, "blue"), 255 }
    end
    -- colours[0] is the player's own; the rest are the build's palette
    local colours = { [0] = rgb(req.color, "colour") }
    local palette = req.palette or {}
    assert(type(palette) == "table" and #palette <= 256, "bad palette")
    for i, c in ipairs(palette) do colours[i] = rgb(c, "palette colour") end
    local material = MATERIALS[req.material] and req.material or "Plastic"
    local owner = owner_of(req.player)

    -- one builder per distinct asset, size and colour; they are costly to make
    local cache = {}
    local function builder(asset, sx, sy, sz, colour)
        local key = asset .. ":" .. sx .. ":" .. sy .. ":" .. sz .. ":" .. colour
        local b = cache[key]
        if not b then
            b = make_builder({ asset = req.assets[asset], size = { sx, sy, sz }, color = colours[colour], material = material, owner = owner })
            cache[key] = b
        end
        return b
    end

    local stack = builds[req.player] or {}
    builds[req.player] = stack
    if req.first or #stack == 0 then stack[#stack + 1] = {} end
    local ids = stack[#stack]

    local d = req.bricks
    local report = transaction(server_player(), function(tx)
        for i = 1, #d, FIELDS do
            local asset = int(d[i], 1, #req.assets, "asset index")
            local sx, sy, sz = int(d[i + 1], 1, 4000, "size"), int(d[i + 2], 1, 4000, "size"), int(d[i + 3], 1, 4000, "size")
            local x, y, z = int(d[i + 4], -1000000, 1000000, "x"), int(d[i + 5], -1000000, 1000000, "y"), int(d[i + 6], -1000000, 1000000, "z")
            local orientation = int(d[i + 7], 0, 23, "orientation")
            local colour = int(d[i + 8], 0, #palette, "colour")
            ids[#ids + 1] = tx.place(builder(asset, sx, sy, sz, colour), { ox + x, oy + y, oz + z }, orientation)
        end
    end)
    return { sent = count, placed = report.BricksPlaced or 0, blocked = report.BricksBlocked or 0, lines = report.lines }
end)

-- remove the player's newest build; returns how many bricks it had
Omegga.handle("arcs_undo", function(player)
    assert(type(player) == "string", "bad request")
    local stack = builds[player]
    if not stack or #stack == 0 then return { removed = 0 } end
    local ids = table.remove(stack)
    transaction(server_player(), function(tx)
        for _, id in ipairs(ids) do tx.delete(id) end
    end)
    return { removed = #ids }
end)
