data modify storage e2e:vars mathx__make_pair.v_tmp set value {}
execute store result storage e2e:vars mathx__make_pair.t_0 int 1 run scoreboard players get #mathx__make_pair.v_a e2e
data modify storage e2e:vars mathx__make_pair.v_tmp.a set from storage e2e:vars mathx__make_pair.t_0
data remove storage e2e:vars mathx__make_pair.t_0
execute store result storage e2e:vars mathx__make_pair.t_0 int 1 run scoreboard players get #mathx__make_pair.v_b e2e
data modify storage e2e:vars mathx__make_pair.v_tmp.b set from storage e2e:vars mathx__make_pair.t_0
data remove storage e2e:vars mathx__make_pair.t_0
data modify storage e2e:ret v set from storage e2e:vars mathx__make_pair.v_tmp
data remove storage e2e:vars mathx__make_pair.v_tmp
return 1
