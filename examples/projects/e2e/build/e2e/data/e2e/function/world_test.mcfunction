kill @e[type=minecraft:marker]
summon minecraft:marker 8 64 8 {Tags:["e2e.owned"]}
summon minecraft:marker 8 64 8 {Tags:["e2e.owned"]}
summon minecraft:marker 8 64 8 {Tags:["e2e.owned"]}
scoreboard players set #world_test.v_n e2e 0
execute as @e[type=minecraft:marker] run function e2e:world_test/b0
scoreboard players operation #g.r_entities e2e = #world_test.v_n e2e
execute if entity @e[type=minecraft:marker] run function e2e:world_test/b1
execute as @e[type=minecraft:marker,limit=2] run function e2e:world_test/b2
scoreboard players set #world_test.v_t e2e 0
execute as @e[type=minecraft:marker] at @s run function e2e:world_test/b3
scoreboard players operation #g.r_tagged e2e = #world_test.v_t e2e
kill @e[type=minecraft:marker]
scoreboard players set #world_test.v_level e2e 1
scoreboard players set #world_test.v_bx e2e 8
scoreboard players set #world_test.v_hv e2e 64
scoreboard players operation #world_test.v_hv e2e += #world_test.v_level e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_hv e2e
execute store result storage e2e:macro h2 int 1 run scoreboard players get #world_test.v_bx e2e
function e2e:world_test/m0 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
data remove storage e2e:macro h2
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_bx e2e
function e2e:world_test/m1 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
function e2e:entity_test
function e2e:string_test
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_bx e2e
function e2e:world_test/m2 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
scoreboard players set #world_test.v_ty e2e 70
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_ty e2e
execute store result storage e2e:macro h2 int 1 run scoreboard players get #world_test.v_bx e2e
function e2e:world_test/m3 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
data remove storage e2e:macro h2
scoreboard players set #world_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_ty e2e
execute store result storage e2e:macro h2 int 1 run scoreboard players get #world_test.v_bx e2e
execute store success score #world_test.t_0 e2e run function e2e:world_test/m4 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
data remove storage e2e:macro h2
scoreboard players operation #world_test.v_ok1 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store success score #world_test.t_0 e2e run teleport @e[tag=nobody,limit=1] 1 2 3
scoreboard players operation #world_test.v_ok2 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_ty e2e
execute store result storage e2e:macro h2 int 1 run scoreboard players get #world_test.v_bx e2e
execute store success score #world_test.t_0 e2e run function e2e:world_test/m5 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
data remove storage e2e:macro h2
scoreboard players operation #world_test.v_ok3 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store success score #world_test.t_0 e2e run tag @e[tag=e2e.owned,limit=1] add mark
scoreboard players operation #world_test.v_ok4 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store success score #world_test.t_0 e2e run tag @e[tag=e2e.owned,limit=1] add mark
scoreboard players operation #world_test.v_ok5 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store success score #world_test.t_0 e2e run kill @e[tag=nobody]
scoreboard players operation #world_test.v_ok6 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_bx e2e
execute store success score #world_test.t_0 e2e run function e2e:world_test/m6 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
scoreboard players operation #world_test.v_ok7 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_bx e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_bx e2e
execute store success score #world_test.t_0 e2e run function e2e:world_test/m7 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
scoreboard players operation #world_test.v_ok8 e2e = #world_test.t_0 e2e
scoreboard players set #world_test.v_res e2e 0
execute if score #world_test.v_ok1 e2e matches 1 run function e2e:world_test/b6
execute if score #world_test.v_ok2 e2e matches 1 run function e2e:world_test/b7
execute if score #world_test.v_ok3 e2e matches 1 run function e2e:world_test/b8
execute if score #world_test.v_ok4 e2e matches 1 run function e2e:world_test/b9
execute if score #world_test.v_ok5 e2e matches 1 run function e2e:world_test/b10
execute if score #world_test.v_ok6 e2e matches 1 run function e2e:world_test/b11
execute if score #world_test.v_ok7 e2e matches 1 run function e2e:world_test/b12
execute if score #world_test.v_ok8 e2e matches 1 run function e2e:world_test/b13
scoreboard players operation #g.r_results e2e = #world_test.v_res e2e
scoreboard players set #world_test.t_0 e2e 0
execute store success score #world_test.t_0 e2e run teleport @e[tag=nobody,limit=1] 0 0 0
execute unless score #world_test.t_0 e2e matches 1 run function e2e:world_test/b14
scoreboard players set #world_test.v_secs e2e 3
scoreboard players operation #world_test.t_0 e2e = #world_test.v_secs e2e
scoreboard players operation #world_test.t_0 e2e *= #c_20 e2e
scoreboard players add #world_test.t_0 e2e 19
scoreboard players operation #world_test.v_hv_2 e2e = #world_test.t_0 e2e
scoreboard players operation #world_test.v_hv_2 e2e /= #c_20 e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #world_test.v_hv_2 e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #world_test.v_level e2e
function e2e:world_test/m8 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
