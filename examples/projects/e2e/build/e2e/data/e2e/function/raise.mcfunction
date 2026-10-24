scoreboard players set #raise.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #raise.v_e e2e
execute store result storage e2e:macro h1 int 1 run scoreboard players get #raise.v_y e2e
execute store success score #raise.t_0 e2e run function e2e:raise/m0 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
scoreboard players operation #ret e2e = #raise.t_0 e2e
return 1
