execute if score #mathx__clamp.v_x e2e < #mathx__clamp.v_lo e2e if function e2e:mathx__clamp/b0 run return 1
execute if score #mathx__clamp.v_x e2e > #mathx__clamp.v_hi e2e if function e2e:mathx__clamp/b1 run return 1
scoreboard players operation #ret e2e = #mathx__clamp.v_x e2e
return 1
