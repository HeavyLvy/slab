scoreboard players set #new_id e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #spawn_at.v_x e2e
function e2e:spawn_at/m0 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #ret e2e = #new_id e2e
return 1
