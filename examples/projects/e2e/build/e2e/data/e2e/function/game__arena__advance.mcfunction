scoreboard players add #g.game__arena__seconds e2e 1
execute store result score #game__arena__advance.t_0 e2e run data get storage e2e:globals game__arena__current.number
scoreboard players add #game__arena__advance.t_0 e2e 1
scoreboard players operation #mathx__clamp.v_x e2e = #game__arena__advance.t_0 e2e
scoreboard players set #mathx__clamp.v_lo e2e 1
scoreboard players set #mathx__clamp.v_hi e2e 5
function e2e:mathx__clamp
scoreboard players operation #game__arena__advance.t_0 e2e = #ret e2e
execute store result storage e2e:vars game__arena__advance.t_0 int 1 run scoreboard players get #game__arena__advance.t_0 e2e
data modify storage e2e:globals game__arena__current.number set from storage e2e:vars game__arena__advance.t_0
data remove storage e2e:vars game__arena__advance.t_0
scoreboard players operation #game__arena__advance.t_0 e2e = #g.game__arena__seconds e2e
scoreboard players operation #game__arena__advance.t_0 e2e *= #c_10 e2e
execute store result storage e2e:vars game__arena__advance.t_0 int 1 run scoreboard players get #game__arena__advance.t_0 e2e
data modify storage e2e:globals game__arena__current.scores append from storage e2e:vars game__arena__advance.t_0
data remove storage e2e:vars game__arena__advance.t_0
